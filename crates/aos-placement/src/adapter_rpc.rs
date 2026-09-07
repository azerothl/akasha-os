//! RPC indépendant pour les runtimes NPU/WebGPU expérimentaux.
//!
//! Le protocole ne connaît ni la stack AMD, ni WebGPU, ni llama.cpp. Un
//! runtime externe peut donc être remplacé sans modifier le Model Subsystem.
//! Les endpoints ne sont jamais ouverts implicitement : le client est créé
//! uniquement par un appelant qui a déjà validé sa configuration.

use crate::adapters::{validate_handshake, AdapterHandshake, ADAPTER_PROTOCOL_VERSION};
use crate::{BackendKind, Quantization};
use serde::{Deserialize, Serialize};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

/// The frame includes the CBOR envelope around a tensor. Keep headroom above
/// the tensor limit so every accepted tensor can actually be transported.
pub const ADAPTER_RPC_MAX_FRAME_BYTES: usize = 20 * 1024 * 1024;
pub const ADAPTER_RPC_MAX_TENSOR_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterExecutionPhase {
    Prefill,
    Decode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdapterRpcMessage {
    Hello {
        protocol_version: u16,
        backend: BackendKind,
    },
    Handshake(AdapterHandshake),
    Execute {
        request_id: String,
        phase: AdapterExecutionPhase,
        operation: String,
        quantization: Quantization,
        tensor: Vec<u8>,
    },
    Result {
        request_id: String,
        output: Vec<u8>,
    },
    Error {
        request_id: Option<String>,
        message: String,
    },
}

pub struct AdapterRpcClient {
    stream: TcpStream,
    backend: BackendKind,
    handshake: AdapterHandshake,
    next_request: u64,
}

impl AdapterRpcClient {
    pub async fn connect(
        endpoint: &str,
        expected_backend: BackendKind,
        expected_memory_bytes: u64,
        expected_operations: &[String],
        expected_quantizations: &[Quantization],
    ) -> Result<Self, String> {
        let address = endpoint
            .strip_prefix("tcp://")
            .or_else(|| endpoint.strip_prefix("akasha://"))
            .ok_or_else(|| "endpoint adaptateur: schéma tcp:// ou akasha:// requis".to_string())?;
        let mut stream = TcpStream::connect(address)
            .await
            .map_err(|error| format!("connexion adaptateur impossible: {error}"))?;
        write_message(
            &mut stream,
            &AdapterRpcMessage::Hello {
                protocol_version: ADAPTER_PROTOCOL_VERSION,
                backend: expected_backend,
            },
        )
        .await
        .map_err(|error| format!("hello adaptateur impossible: {error}"))?;
        let response = read_message(&mut stream)
            .await
            .map_err(|error| format!("handshake adaptateur illisible: {error}"))?;
        let AdapterRpcMessage::Handshake(handshake) = response else {
            return Err("réponse de handshake adaptateur inattendue".into());
        };
        validate_handshake(
            expected_backend,
            expected_memory_bytes,
            expected_operations,
            expected_quantizations,
            &handshake,
        )?;
        Ok(Self {
            stream,
            backend: expected_backend,
            handshake,
            next_request: 0,
        })
    }

    pub fn backend(&self) -> BackendKind {
        self.backend
    }

    pub fn handshake(&self) -> &AdapterHandshake {
        &self.handshake
    }

    pub async fn execute(
        &mut self,
        operation: &str,
        quantization: Quantization,
        tensor: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        self.execute_phase(
            AdapterExecutionPhase::Decode,
            operation,
            quantization,
            tensor,
        )
        .await
    }

    pub async fn execute_phase(
        &mut self,
        phase: AdapterExecutionPhase,
        operation: &str,
        quantization: Quantization,
        tensor: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        if operation.trim().is_empty() {
            return Err("opération adaptateur vide".into());
        }
        if tensor.is_empty() || tensor.len() > ADAPTER_RPC_MAX_TENSOR_BYTES {
            return Err("tenseur adaptateur vide ou trop volumineux".into());
        }
        self.next_request = self.next_request.wrapping_add(1);
        let request_id = format!("adapter-{}", self.next_request);
        write_message(
            &mut self.stream,
            &AdapterRpcMessage::Execute {
                request_id: request_id.clone(),
                phase,
                operation: operation.to_string(),
                quantization,
                tensor,
            },
        )
        .await
        .map_err(|error| format!("requête adaptateur impossible: {error}"))?;
        match read_message(&mut self.stream)
            .await
            .map_err(|error| format!("réponse adaptateur illisible: {error}"))?
        {
            AdapterRpcMessage::Result {
                request_id: response_id,
                output,
            } if response_id == request_id => {
                if output.is_empty() || output.len() > ADAPTER_RPC_MAX_TENSOR_BYTES {
                    Err("résultat adaptateur vide ou trop volumineux".into())
                } else {
                    Ok(output)
                }
            }
            AdapterRpcMessage::Error {
                request_id: Some(id),
                message,
            } if id == request_id => Err(format!("runtime adaptateur: {message}")),
            AdapterRpcMessage::Error {
                request_id: None,
                message,
            } => Err(format!("runtime adaptateur: {message}")),
            _ => Err("réponse adaptateur non corrélée".into()),
        }
    }
}

async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &AdapterRpcMessage,
) -> io::Result<()> {
    let mut encoded = Vec::new();
    ciborium::into_writer(message, &mut encoded)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    if encoded.is_empty() || encoded.len() > ADAPTER_RPC_MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "trame adaptateur hors limites",
        ));
    }
    writer
        .write_all(&(encoded.len() as u32).to_be_bytes())
        .await?;
    writer.write_all(&encoded).await?;
    writer.flush().await
}

async fn read_message<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<AdapterRpcMessage> {
    let mut length = [0u8; 4];
    reader.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > ADAPTER_RPC_MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trame adaptateur hors limites",
        ));
    }
    let mut encoded = vec![0u8; length];
    reader.read_exact(&mut encoded).await?;
    ciborium::from_reader(encoded.as_slice())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn fake_runtime_handshake_and_execute_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            assert!(matches!(
                read_message(&mut stream).await.unwrap(),
                AdapterRpcMessage::Hello {
                    backend: BackendKind::Npu,
                    ..
                }
            ));
            write_message(
                &mut stream,
                &AdapterRpcMessage::Handshake(AdapterHandshake {
                    protocol_version: ADAPTER_PROTOCOL_VERSION,
                    backend: BackendKind::Npu,
                    device: "fake-npu".into(),
                    memory_bytes: 4096,
                    supported_operations: vec!["gemm".into()],
                    supported_quantizations: vec![Quantization::Q4],
                }),
            )
            .await
            .unwrap();
            let AdapterRpcMessage::Execute {
                request_id,
                phase,
                tensor,
                ..
            } = read_message(&mut stream).await.unwrap()
            else {
                panic!("requête execute attendue");
            };
            assert_eq!(phase, AdapterExecutionPhase::Decode);
            write_message(
                &mut stream,
                &AdapterRpcMessage::Result {
                    request_id,
                    output: tensor,
                },
            )
            .await
            .unwrap();
        });
        let mut client = AdapterRpcClient::connect(
            &format!("tcp://{address}"),
            BackendKind::Npu,
            1024,
            &["gemm".into()],
            &[Quantization::Q4],
        )
        .await
        .unwrap();
        assert_eq!(client.backend(), BackendKind::Npu);
        assert_eq!(
            client
                .execute("gemm", Quantization::Q4, vec![1, 2, 3])
                .await
                .unwrap(),
            vec![1, 2, 3]
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn execute_rejects_empty_and_oversized_tensors_before_network_io() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = read_message(&mut stream).await.unwrap();
            write_message(
                &mut stream,
                &AdapterRpcMessage::Handshake(AdapterHandshake {
                    protocol_version: ADAPTER_PROTOCOL_VERSION,
                    backend: BackendKind::Npu,
                    device: "fake".into(),
                    memory_bytes: 1,
                    supported_operations: vec!["gemm".into()],
                    supported_quantizations: vec![Quantization::Q4],
                }),
            )
            .await
            .unwrap();
        });
        let client = AdapterRpcClient::connect(
            &format!("tcp://{address}"),
            BackendKind::Npu,
            1,
            &["gemm".into()],
            &[Quantization::Q4],
        )
        .await;
        let mut client = client.unwrap();
        assert!(client
            .execute("gemm", Quantization::Q4, Vec::new())
            .await
            .is_err());
        assert!(client
            .execute(
                "gemm",
                Quantization::Q4,
                vec![0; ADAPTER_RPC_MAX_TENSOR_BYTES + 1]
            )
            .await
            .is_err());
        server.abort();
        let _ = server.await;
    }
}
