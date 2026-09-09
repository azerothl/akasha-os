//! Entrée/sortie minimale d'un modèle GGUF pour l'adaptateur RPC Akasha.
//!
//! Le chemin est volontairement indépendant de llama.cpp : il fournit les
//! embeddings et les logits autour du pipeline de couches. Les lignes sont
//! décodées à la demande afin de ne pas matérialiser la matrice vocabulaire.

use crate::{CpuLayerExecutor, F32Tensor, GgufModel};

#[derive(Debug, Clone)]
pub struct CpuGgufModelIo {
    model: GgufModel,
    embedding_name: String,
    output_name: String,
    output_norm: F32Tensor,
    hidden: usize,
    vocab: usize,
    eps: f32,
}

impl CpuGgufModelIo {
    pub fn from_gguf(model: GgufModel) -> Result<Self, String> {
        let embedding_name = find_tensor(&model, &["token_embd.weight", "tok_embeddings.weight"])?;
        let embedding = model
            .tensors
            .iter()
            .find(|tensor| tensor.name == embedding_name)
            .ok_or("tenseur d'embedding GGUF absent")?;
        if embedding.dimensions.len() != 2 {
            return Err("embedding GGUF non matriciel".into());
        }
        let hidden = usize::try_from(embedding.dimensions[0])
            .map_err(|_| "dimension hidden GGUF trop grande".to_string())?;
        let vocab = usize::try_from(embedding.dimensions[1])
            .map_err(|_| "vocabulaire GGUF trop grand".to_string())?;
        let output_name =
            find_tensor(&model, &["output.weight"]).unwrap_or_else(|_| embedding_name.clone());
        let output = model
            .tensors
            .iter()
            .find(|tensor| tensor.name == output_name)
            .ok_or("tenseur de sortie GGUF absent")?;
        if output.dimensions != embedding.dimensions {
            return Err("embedding et projection de sortie GGUF incompatibles".into());
        }
        let norm_name = find_tensor(&model, &["output_norm.weight", "norm.weight"])?;
        let output_norm = model.tensor_f32_matrix(&norm_name)?;
        if output_norm.shape != vec![hidden as u32] {
            return Err("norme de sortie GGUF incompatible".into());
        }
        let eps = model
            .metadata_f32("llama.attention.layer_norm_rms_epsilon")
            .unwrap_or(1e-5);
        if !eps.is_finite() || eps <= 0.0 {
            return Err("epsilon RMSNorm GGUF invalide".into());
        }
        Ok(Self {
            model,
            embedding_name,
            output_name,
            output_norm,
            hidden,
            vocab,
            eps,
        })
    }

    pub fn hidden_size(&self) -> usize {
        self.hidden
    }

    pub fn vocab_size(&self) -> usize {
        self.vocab
    }

    pub fn file_len(&self) -> u64 {
        self.model.file_len()
    }

    pub fn layer_data_ranges(
        &self,
        first_layer: u32,
        last_layer: u32,
    ) -> Result<Vec<(u64, u64)>, String> {
        self.model.layer_data_ranges(first_layer, last_layer)
    }

    pub fn embedding(&self, token_id: u32) -> Result<F32Tensor, String> {
        let token =
            usize::try_from(token_id).map_err(|_| "identifiant de token invalide".to_string())?;
        if token >= self.vocab {
            return Err("identifiant de token hors vocabulaire".into());
        }
        F32Tensor::new(
            vec![self.hidden as u32, 1],
            self.model
                .tensor_f32_weight_row(&self.embedding_name, token)?,
        )
    }

    /// Calcule les logits d'un seul état caché sans charger toute la tête.
    pub fn logits(&self, hidden: &F32Tensor) -> Result<Vec<f32>, String> {
        if hidden.shape != vec![self.hidden as u32, 1] {
            return Err("état caché incompatible avec la sortie GGUF".into());
        }
        let normalized = CpuLayerExecutor::rms_norm(hidden, self.eps)?;
        let weighted: Vec<f32> = normalized
            .values
            .iter()
            .zip(self.output_norm.values.iter())
            .map(|(value, weight)| value * weight)
            .collect();
        let mut logits = Vec::with_capacity(self.vocab);
        for token in 0..self.vocab {
            let row = self.model.tensor_f32_weight_row(&self.output_name, token)?;
            logits.push(row.iter().zip(weighted.iter()).map(|(a, b)| a * b).sum());
        }
        Ok(logits)
    }
}

fn find_tensor(model: &GgufModel, names: &[&str]) -> Result<String, String> {
    names
        .iter()
        .find(|name| model.tensors.iter().any(|tensor| tensor.name == **name))
        .map(|name| (*name).to_string())
        .ok_or_else(|| format!("tenseur GGUF absent: {}", names.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&(value.len() as u64).to_le_bytes());
        out.extend_from_slice(value.as_bytes());
    }

    fn tiny_model() -> GgufModel {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&3u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        for (name, dimensions, offset) in [
            ("token_embd.weight", vec![2u64, 2], 0u64),
            ("output.weight", vec![2u64, 2], 16u64),
            ("output_norm.weight", vec![2u64], 32u64),
        ] {
            string(&mut bytes, name);
            bytes.extend_from_slice(&(dimensions.len() as u32).to_le_bytes());
            for dimension in dimensions {
                bytes.extend_from_slice(&dimension.to_le_bytes());
            }
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        while bytes.len() % 32 != 0 {
            bytes.push(0);
        }
        for value in [1.0f32, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 1.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        GgufModel::from_bytes(bytes).unwrap()
    }

    #[test]
    fn refuse_un_modele_sans_embedding() {
        let model = GgufModel::from_bytes({
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"GGUF");
            bytes.extend_from_slice(&3u32.to_le_bytes());
            bytes.extend_from_slice(&0u64.to_le_bytes());
            bytes.extend_from_slice(&0u64.to_le_bytes());
            while bytes.len() % 32 != 0 {
                bytes.push(0);
            }
            bytes
        })
        .unwrap();
        assert!(CpuGgufModelIo::from_gguf(model).is_err());
    }

    #[test]
    fn construit_embedding_et_logits_sans_materiel_complet() {
        let io = CpuGgufModelIo::from_gguf(tiny_model()).unwrap();
        assert_eq!(io.hidden_size(), 2);
        assert_eq!(io.vocab_size(), 2);
        assert_eq!(io.embedding(1).unwrap().values, vec![3.0, 4.0]);
        let logits = io
            .logits(&F32Tensor::new(vec![2, 1], vec![3.0, 4.0]).unwrap())
            .unwrap();
        assert_eq!(logits.len(), 2);
        assert!(logits.iter().all(|value| value.is_finite()));
    }
}
