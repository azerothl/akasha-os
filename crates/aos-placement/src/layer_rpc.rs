//! Adaptateur RPC de couches indépendant de llama.cpp.
//!
//! Ce module ne connaît ni les tenseurs internes de ggml ni les symboles
//! d'une version donnée de llama.cpp. Il fournit le contrat Akasha : une
//! activation opaque, paginée et authentifiée par le canal LAN, puis un
//! exécuteur local injectable côté worker.

use crate::distributed::{LanActivationAssembly, LanWorkMessage};

pub const LAYER_RPC_PROTOCOL_VERSION: u16 = 1;
pub const LAYER_RPC_PAGE_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerRpcRequest {
    pub request_id: String,
    pub work_id: String,
    pub shard_id: u32,
    pub layer_index: u32,
    pub sequence: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerRpcResult {
    pub request_id: String,
    pub work_id: String,
    pub shard_id: u32,
    pub layer_index: u32,
    pub sequence: u32,
    pub bytes: Vec<u8>,
}

pub trait LayerRpcExecutor {
    fn execute(&mut self, request: LayerRpcRequest) -> Result<LayerRpcResult, String>;
}

/// Accumule une activation entrante puis l'exécute exactement une fois.
#[derive(Debug)]
pub struct LayerRpcReceiver {
    assembly: LanActivationAssembly,
    work_id: String,
}

impl LayerRpcReceiver {
    pub fn new(
        work_id: impl Into<String>,
        request_id: impl Into<String>,
        shard_id: u32,
        layer_index: u32,
        sequence: u32,
        total_bytes: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            assembly: LanActivationAssembly::new(
                request_id,
                shard_id,
                layer_index,
                sequence,
                total_bytes,
            )?,
            work_id: work_id.into(),
        })
    }

    pub fn push_page<E: LayerRpcExecutor>(
        &mut self,
        page_index: u32,
        data: &[u8],
        final_page: bool,
        executor: &mut E,
    ) -> Result<Option<Vec<LanWorkMessage>>, String> {
        let Some(bytes) = self.assembly.push_page(page_index, data, final_page)? else {
            return Ok(None);
        };
        let request = LayerRpcRequest {
            request_id: self.assembly.request_id.clone(),
            work_id: self.work_id.clone(),
            shard_id: self.assembly.shard_id,
            layer_index: self.assembly.layer_index,
            sequence: self.assembly.sequence,
            bytes,
        };
        let result = executor.execute(request)?;
        if result.work_id != self.work_id
            || result.request_id != self.assembly.request_id
            || result.shard_id != self.assembly.shard_id
            || result.layer_index != self.assembly.layer_index
            || result.sequence != self.assembly.sequence
            || result.bytes.is_empty()
            || result.bytes.len() > 64 * 1024 * 1024
        {
            return Err("résultat de l'adaptateur RPC Akasha incohérent".into());
        }
        Ok(Some(page_result(result)))
    }
}

fn page_result(result: LayerRpcResult) -> Vec<LanWorkMessage> {
    result
        .bytes
        .chunks(LAYER_RPC_PAGE_BYTES)
        .enumerate()
        .map(|(page_index, data)| LanWorkMessage::LayerActivationResult {
            work_id: result.work_id.clone(),
            request_id: result.request_id.clone(),
            shard_id: result.shard_id,
            layer_index: result.layer_index,
            sequence: result.sequence,
            page_index: page_index as u32,
            total_bytes: result.bytes.len() as u64,
            data: data.to_vec(),
            final_page: page_index + 1 == result.bytes.len().div_ceil(LAYER_RPC_PAGE_BYTES),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;

    impl LayerRpcExecutor for Echo {
        fn execute(&mut self, mut request: LayerRpcRequest) -> Result<LayerRpcResult, String> {
            request.bytes.reverse();
            Ok(LayerRpcResult {
                request_id: request.request_id,
                work_id: request.work_id,
                shard_id: request.shard_id,
                layer_index: request.layer_index,
                sequence: request.sequence,
                bytes: request.bytes,
            })
        }
    }

    #[test]
    fn receiver_execute_et_page_un_result() {
        let mut receiver = LayerRpcReceiver::new("work", "req", 2, 7, 1, 4).unwrap();
        let mut echo = Echo;
        assert!(receiver
            .push_page(0, b"ab", false, &mut echo)
            .unwrap()
            .is_none());
        let result = receiver
            .push_page(1, b"cd", true, &mut echo)
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 1);
        assert!(matches!(
            &result[0],
            LanWorkMessage::LayerActivationResult {
                page_index: 0,
                final_page: true,
                data,
                ..
            } if data == b"dcba"
        ));
    }
}
