//! Format de tenseur Akasha pour l'adaptateur RPC indépendant.

const MAGIC: &[u8; 4] = b"AKT1";
const MAX_RANK: usize = 4;
const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorDType {
    F32 = 1,
}

#[derive(Debug, Clone, PartialEq)]
pub struct F32Tensor {
    pub shape: Vec<u32>,
    pub values: Vec<f32>,
}

impl F32Tensor {
    pub fn new(shape: Vec<u32>, values: Vec<f32>) -> Result<Self, String> {
        if shape.is_empty() || shape.len() > MAX_RANK || shape.contains(&0) {
            return Err("forme de tenseur Akasha invalide".into());
        }
        let expected = shape.iter().try_fold(1usize, |acc, &d| acc.checked_mul(d as usize));
        if expected != Some(values.len()) || values.len().saturating_mul(4) > MAX_BYTES {
            return Err("taille de tenseur Akasha invalide".into());
        }
        if values.iter().any(|v| !v.is_finite()) {
            return Err("tenseur Akasha non fini".into());
        }
        Ok(Self { shape, values })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.shape.len() * 4 + self.values.len() * 4);
        out.extend_from_slice(MAGIC);
        out.push(TensorDType::F32 as u8);
        out.push(self.shape.len() as u8);
        out.extend_from_slice(&0u16.to_le_bytes());
        for &dim in &self.shape {
            out.extend_from_slice(&dim.to_le_bytes());
        }
        out.extend_from_slice(&(self.values.len() as u64).to_le_bytes());
        for value in &self.values {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }

    pub fn decode(input: &[u8]) -> Result<Self, String> {
        if input.len() < 16 || &input[..4] != MAGIC || input[4] != TensorDType::F32 as u8 {
            return Err("en-tête de tenseur Akasha invalide".into());
        }
        let rank = input[5] as usize;
        if rank == 0 || rank > MAX_RANK {
            return Err("rang de tenseur Akasha invalide".into());
        }
        let mut cursor = 8usize;
        let mut shape = Vec::with_capacity(rank);
        for _ in 0..rank {
            let end = cursor + 4;
            let bytes = input.get(cursor..end).ok_or("forme de tenseur tronquée")?;
            let dim = u32::from_le_bytes(bytes.try_into().unwrap());
            if dim == 0 {
                return Err("dimension de tenseur Akasha nulle".into());
            }
            shape.push(dim);
            cursor = end;
        }
        let count_bytes = input.get(cursor..cursor + 8).ok_or("tenseur tronqué")?;
        let count = u64::from_le_bytes(count_bytes.try_into().unwrap()) as usize;
        cursor += 8;
        let payload = count.checked_mul(4).ok_or("tenseur trop grand")?;
        if payload > MAX_BYTES || input.len() != cursor + payload {
            return Err("payload de tenseur Akasha invalide".into());
        }
        let bytes = &input[cursor..];
        let (chunks, remainder) = bytes.as_chunks();
        if !remainder.is_empty() {
            return Err("payload de tenseur Akasha invalide".into());
        }
        let values = chunks
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        Self::new(shape, values)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CpuLayerExecutor {
    pub weights: F32Tensor,
}

impl CpuLayerExecutor {
    /// Référence déterministe : X [in, tokens] → W [out, in] × X.
    pub fn matmul(&self, input: &F32Tensor) -> Result<F32Tensor, String> {
        if self.weights.shape.len() != 2 || input.shape.len() != 2 {
            return Err("matmul Akasha attend deux matrices".into());
        }
        let (out, inner) = (self.weights.shape[0] as usize, self.weights.shape[1] as usize);
        let (input_inner, tokens) = (input.shape[0] as usize, input.shape[1] as usize);
        if inner != input_inner || self.weights.values.len() != out * inner {
            return Err("dimensions matmul Akasha incompatibles".into());
        }
        let mut values = vec![0.0; out * tokens];
        for o in 0..out {
            for t in 0..tokens {
                values[o * tokens + t] = (0..inner)
                    .map(|i| self.weights.values[o * inner + i] * input.values[i * tokens + t])
                    .sum();
            }
        }
        F32Tensor::new(vec![out as u32, tokens as u32], values)
    }

    /// RMSNorm de référence sur la première dimension (hidden), par colonne.
    pub fn rms_norm(input: &F32Tensor, eps: f32) -> Result<F32Tensor, String> {
        if input.shape.is_empty() || input.shape[0] == 0 || !eps.is_finite() || eps <= 0.0 {
            return Err("paramètres RMSNorm Akasha invalides".into());
        }
        let hidden = input.shape[0] as usize;
        let columns = input.values.len() / hidden;
        let mut values = input.values.clone();
        for column in 0..columns {
            let start = column * hidden;
            let mean_square = values[start..start + hidden]
                .iter()
                .map(|value| value * value)
                .sum::<f32>()
                / hidden as f32;
            let scale = (mean_square + eps).sqrt().recip();
            for value in &mut values[start..start + hidden] {
                *value *= scale;
            }
        }
        F32Tensor::new(input.shape.clone(), values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenseur_roundtrip_et_matmul_cpu() {
        let input = F32Tensor::new(vec![2, 1], vec![2.0, 3.0]).unwrap();
        let encoded = input.encode();
        assert_eq!(F32Tensor::decode(&encoded).unwrap(), input);
        let executor = CpuLayerExecutor {
            weights: F32Tensor::new(vec![1, 2], vec![4.0, 5.0]).unwrap(),
        };
        assert_eq!(executor.matmul(&input).unwrap().values, vec![23.0]);
        let normalized = CpuLayerExecutor::rms_norm(&input, 1e-5).unwrap();
        assert!((normalized.values[0] - 0.7845).abs() < 1e-3);
    }

    #[test]
    fn tenseur_refuse_payload_tronque_ou_non_fini() {
        let tensor = F32Tensor::new(vec![1], vec![1.0]).unwrap();
        assert!(F32Tensor::decode(&tensor.encode()[..tensor.encode().len() - 1]).is_err());
        assert!(F32Tensor::new(vec![1], vec![f32::NAN]).is_err());
    }
}
