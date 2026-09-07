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
        let expected = shape
            .iter()
            .try_fold(1usize, |acc, &d| acc.checked_mul(d as usize));
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
        let values = input[cursor..]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        Self::new(shape, values)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CpuLayerExecutor {
    pub weights: F32Tensor,
}

#[derive(Debug, Clone)]
pub struct CpuTransformerBlock {
    pub norm_attn: F32Tensor,
    pub q_proj: F32Tensor,
    pub k_proj: F32Tensor,
    pub v_proj: F32Tensor,
    pub o_proj: F32Tensor,
    pub norm_ffn: F32Tensor,
    pub gate_proj: F32Tensor,
    pub up_proj: F32Tensor,
    pub down_proj: F32Tensor,
    pub n_heads: usize,
    pub n_kv_heads: usize,
    pub eps: f32,
}

#[derive(Debug, Clone)]
pub struct CpuKvCache {
    pub keys: F32Tensor,
    pub values: F32Tensor,
    pub max_tokens: u32,
    token_count: u32,
}

impl CpuKvCache {
    pub fn new(n_kv_heads: usize, head_dim: usize, max_tokens: u32) -> Result<Self, String> {
        if n_kv_heads == 0 || head_dim == 0 || max_tokens == 0 {
            return Err("paramètres cache KV invalides".into());
        }
        Ok(Self {
            keys: F32Tensor::new(vec![(n_kv_heads * head_dim) as u32, 1], vec![0.0; n_kv_heads * head_dim])?,
            values: F32Tensor::new(vec![(n_kv_heads * head_dim) as u32, 1], vec![0.0; n_kv_heads * head_dim])?,
            max_tokens,
            token_count: 0,
        })
    }

    pub fn len(&self) -> usize {
        self.token_count as usize
    }

    pub fn is_empty(&self) -> bool {
        self.token_count == 0
    }

    fn append(&mut self, key: &F32Tensor, value: &F32Tensor) -> Result<(), String> {
        if key.shape != value.shape
            || key.shape.len() != 2
            || key.shape[1] != 1
            || self.len() >= self.max_tokens as usize
        {
            return Err("ajout au cache KV impossible".into());
        }
        let hidden = key.shape[0] as usize;
        if self.keys.shape[0] != key.shape[0] {
            return Err("dimensions cache KV incompatibles".into());
        }
        let old_len = self.len();
        let mut keys = Vec::with_capacity(hidden * (old_len + 1));
        let mut values = Vec::with_capacity(hidden * (old_len + 1));
        for row in 0..hidden {
            keys.extend_from_slice(&self.keys.values[row * old_len..row * old_len + old_len]);
            keys.push(key.values[row]);
            values.extend_from_slice(&self.values.values[row * old_len..row * old_len + old_len]);
            values.push(value.values[row]);
        }
        self.keys = F32Tensor::new(vec![hidden as u32, (old_len + 1) as u32], keys)?;
        self.values = F32Tensor::new(vec![hidden as u32, (old_len + 1) as u32], values)?;
        self.token_count = self.token_count.saturating_add(1);
        Ok(())
    }
}

impl CpuTransformerBlock {
    /// Construit un bloc Llama à partir des noms de tenseurs GGUF standard.
    /// Les dimensions GGUF sont conservées dans l'ordre du fichier; le format
    /// de calcul Akasha attend les poids sous forme [out, in].
    pub fn from_gguf(
        model: &crate::gguf::GgufModel,
        layer: usize,
        n_heads: usize,
        n_kv_heads: usize,
        eps: f32,
    ) -> Result<Self, String> {
        let weight = |suffix: &str| model.tensor_f32_weight(&format!("blk.{layer}.{suffix}"));
        let tensor = |suffix: &str| model.tensor_f32_matrix(&format!("blk.{layer}.{suffix}"));
        let vector = |suffix: &str| {
            let value = tensor(suffix)?;
            if value.shape.len() != 1 {
                return Err(format!("poids GGUF non vectoriel: blk.{layer}.{suffix}"));
            }
            Ok(value)
        };
        let block = Self {
            norm_attn: vector("attn_norm.weight")?,
            q_proj: weight("attn_q.weight")?,
            k_proj: weight("attn_k.weight")?,
            v_proj: weight("attn_v.weight")?,
            o_proj: weight("attn_output.weight")?,
            norm_ffn: vector("ffn_norm.weight")?,
            gate_proj: weight("ffn_gate.weight")?,
            up_proj: weight("ffn_up.weight")?,
            down_proj: weight("ffn_down.weight")?,
            n_heads,
            n_kv_heads,
            eps,
        };
        if n_heads == 0 || n_kv_heads == 0 || !n_heads.is_multiple_of(n_kv_heads) {
            return Err("configuration GQA GGUF invalide".into());
        }
        Ok(block)
    }

    /// Variante qui dérive la configuration d'attention des métadonnées Llama.
    pub fn from_gguf_auto(model: &crate::gguf::GgufModel, layer: usize) -> Result<Self, String> {
        let required_u32 = |key: &str| {
            model
                .metadata_u32(key)
                .ok_or_else(|| format!("métadonnée GGUF absente ou invalide: {key}"))
                .map(|value| value as usize)
        };
        let n_heads = required_u32("llama.attention.head_count")?;
        let n_kv_heads = model
            .metadata_u32("llama.attention.head_count_kv")
            .or_else(|| model.metadata_u32("llama.attention.head_count"))
            .map(|value| value as usize)
            .ok_or_else(|| {
                "métadonnée GGUF absente ou invalide: llama.attention.head_count_kv".to_string()
            })?;
        let eps = model
            .metadata_f32("llama.attention.layer_norm_rms_epsilon")
            .unwrap_or(1e-5);
        Self::from_gguf(model, layer, n_heads, n_kv_heads, eps)
    }

    /// Exécute un bloc causal F32 sur un tenseur [hidden, tokens].
    /// Les poids sont stockés sous forme [out, in], sans biais.
    pub fn forward(&self, input: &F32Tensor) -> Result<F32Tensor, String> {
        let tokens = input.shape.get(1).copied().unwrap_or(0);
        let positions: Vec<u32> = (0..tokens).collect();
        self.forward_with_positions(input, &positions, None)
    }

    /// Exécute le bloc avec des positions absolues et, si fourni, RoPE Llama.
    /// `rope_theta=None` conserve le chemin de référence sans rotation.
    pub fn forward_with_positions(
        &self,
        input: &F32Tensor,
        positions: &[u32],
        rope_theta: Option<f32>,
    ) -> Result<F32Tensor, String> {
        let hidden = input
            .shape
            .first()
            .copied()
            .ok_or("entrée Transformer vide")? as usize;
        let tokens = input
            .shape
            .get(1)
            .copied()
            .ok_or("entrée non matricielle")? as usize;
        if positions.len() != tokens {
            return Err("positions Transformer incompatibles".into());
        }
        if hidden == 0 || tokens == 0 || self.n_heads == 0 || self.n_kv_heads == 0 {
            return Err("dimensions Transformer invalides".into());
        }
        if !hidden.is_multiple_of(self.n_heads)
            || !self.n_heads.is_multiple_of(self.n_kv_heads)
        {
            return Err("têtes Transformer incompatibles".into());
        }
        let head_dim = hidden / self.n_heads;
        let normed = rms_norm_weighted(input, &self.norm_attn, self.eps)?;
        let mut q = linear(&self.q_proj, &normed)?;
        let mut k = linear(&self.k_proj, &normed)?;
        let v = linear(&self.v_proj, &normed)?;
        let expected_q = self.n_heads * head_dim;
        let expected_kv = self.n_kv_heads * head_dim;
        if q.shape[0] as usize != expected_q
            || k.shape[0] as usize != expected_kv
            || v.shape[0] as usize != expected_kv
        {
            return Err("dimensions des projections QKV incompatibles".into());
        }
        if let Some(theta) = rope_theta {
            apply_rope(&mut q, self.n_heads, head_dim, positions, theta)?;
            apply_rope(&mut k, self.n_kv_heads, head_dim, positions, theta)?;
        }
        let attended = causal_attention(&q, &k, &v, self.n_heads, self.n_kv_heads, head_dim)?;
        let projected = linear(&self.o_proj, &attended)?;
        let first_residual = add(input, &projected)?;
        let normed = rms_norm_weighted(&first_residual, &self.norm_ffn, self.eps)?;
        let gate = linear(&self.gate_proj, &normed)?;
        let up = linear(&self.up_proj, &normed)?;
        let activated = F32Tensor::new(
            gate.shape.clone(),
            gate.values
                .iter()
                .zip(up.values.iter())
                .map(|(gate, up)| silu(*gate) * up)
                .collect(),
        )?;
        let feed_forward = linear(&self.down_proj, &activated)?;
        add(&first_residual, &feed_forward)
    }

    /// Décodage d'un token avec réutilisation du cache K/V de cette couche.
    pub fn decode_with_cache(
        &self,
        input: &F32Tensor,
        position: u32,
        cache: &mut CpuKvCache,
        rope_theta: Option<f32>,
    ) -> Result<F32Tensor, String> {
        let hidden = input.shape.first().copied().ok_or("entrée Transformer vide")? as usize;
        if input.shape != vec![hidden as u32, 1]
            || self.n_heads == 0
            || self.n_kv_heads == 0
            || !hidden.is_multiple_of(self.n_heads)
            || !self.n_heads.is_multiple_of(self.n_kv_heads)
        {
            return Err("entrée de décodage Transformer invalide".into());
        }
        let head_dim = hidden / self.n_heads;
        let normed = rms_norm_weighted(input, &self.norm_attn, self.eps)?;
        let mut q = linear(&self.q_proj, &normed)?;
        let mut k = linear(&self.k_proj, &normed)?;
        let v = linear(&self.v_proj, &normed)?;
        if q.shape[0] as usize != self.n_heads * head_dim
            || k.shape[0] as usize != self.n_kv_heads * head_dim
            || v.shape != k.shape
        {
            return Err("dimensions QKV du décodage incompatibles".into());
        }
        if let Some(theta) = rope_theta {
            apply_rope(&mut q, self.n_heads, head_dim, &[position], theta)?;
            apply_rope(&mut k, self.n_kv_heads, head_dim, &[position], theta)?;
        }
        if cache.keys.shape[0] != k.shape[0] {
            return Err("cache KV associé à une autre couche".into());
        }
        cache.append(&k, &v)?;
        let attended = decode_attention(&q, &cache.keys, &cache.values, self.n_heads, self.n_kv_heads, head_dim)?;
        let projected = linear(&self.o_proj, &attended)?;
        let first_residual = add(input, &projected)?;
        let normed = rms_norm_weighted(&first_residual, &self.norm_ffn, self.eps)?;
        let gate = linear(&self.gate_proj, &normed)?;
        let up = linear(&self.up_proj, &normed)?;
        let activated = F32Tensor::new(
            gate.shape.clone(),
            gate.values.iter().zip(up.values.iter()).map(|(gate, up)| silu(*gate) * up).collect(),
        )?;
        let feed_forward = linear(&self.down_proj, &activated)?;
        add(&first_residual, &feed_forward)
    }

    /// Prefill causal avec alimentation du cache K/V. Chaque colonne est
    /// traitée dans l'ordre afin de conserver l'attention causale exacte.
    pub fn prefill_with_cache(
        &self,
        input: &F32Tensor,
        position_start: u32,
        cache: &mut CpuKvCache,
        rope_theta: Option<f32>,
    ) -> Result<F32Tensor, String> {
        let hidden = input.shape.first().copied().ok_or("entrée Transformer vide")? as usize;
        let tokens = input.shape.get(1).copied().ok_or("entrée non matricielle")? as usize;
        if hidden == 0 || tokens == 0 || input.values.len() != hidden * tokens {
            return Err("entrée de prefill Transformer invalide".into());
        }
        let mut output = Vec::with_capacity(input.values.len());
        for token in 0..tokens {
            let column = (0..hidden)
                .map(|row| input.values[row * tokens + token])
                .collect();
            let one = F32Tensor::new(vec![hidden as u32, 1], column)?;
            let result = self.decode_with_cache(
                &one,
                position_start.saturating_add(token as u32),
                cache,
                rope_theta,
            )?;
            for row in 0..hidden {
                output.push(result.values[row]);
            }
        }
        F32Tensor::new(vec![hidden as u32, tokens as u32], output)
    }
}

fn linear(weights: &F32Tensor, input: &F32Tensor) -> Result<F32Tensor, String> {
    if weights.shape.len() != 2 || input.shape.len() != 2 || weights.shape[1] != input.shape[0] {
        return Err("projection Transformer incompatible".into());
    }
    CpuLayerExecutor {
        weights: weights.clone(),
    }
    .matmul(input)
}

fn add(left: &F32Tensor, right: &F32Tensor) -> Result<F32Tensor, String> {
    if left.shape != right.shape {
        return Err("résidus Transformer incompatibles".into());
    }
    F32Tensor::new(
        left.shape.clone(),
        left.values
            .iter()
            .zip(right.values.iter())
            .map(|(left, right)| left + right)
            .collect(),
    )
}

fn rms_norm_weighted(input: &F32Tensor, weight: &F32Tensor, eps: f32) -> Result<F32Tensor, String> {
    if weight.shape != vec![input.shape[0]] || !eps.is_finite() || eps <= 0.0 {
        return Err("poids RMSNorm Transformer invalides".into());
    }
    let normalized = CpuLayerExecutor::rms_norm(input, eps)?;
    let hidden = input.shape[0] as usize;
    F32Tensor::new(
        input.shape.clone(),
        normalized
            .values
            .chunks_exact(hidden)
            .flat_map(|column| {
                column
                    .iter()
                    .enumerate()
                    .map(|(i, value)| value * weight.values[i])
            })
            .collect(),
    )
}

fn apply_rope(
    tensor: &mut F32Tensor,
    n_heads: usize,
    head_dim: usize,
    positions: &[u32],
    theta: f32,
) -> Result<(), String> {
    if tensor.shape.len() != 2
        || tensor.shape[0] as usize != n_heads.saturating_mul(head_dim)
        || head_dim < 2
        || !head_dim.is_multiple_of(2)
        || !theta.is_finite()
        || theta <= 1.0
        || positions.len() != tensor.shape[1] as usize
    {
        return Err("paramètres RoPE Transformer invalides".into());
    }
    let tokens = positions.len();
    for head in 0..n_heads {
        for pair in (0..head_dim).step_by(2) {
            let frequency = theta.powf(-(pair as f32) / head_dim as f32);
            for (token, &position) in positions.iter().enumerate() {
                let angle = position as f32 * frequency;
                let (sin, cos) = angle.sin_cos();
                let left_index = (head * head_dim + pair) * tokens + token;
                let right_index = left_index + tokens;
                let left = tensor.values[left_index];
                let right = tensor.values[right_index];
                tensor.values[left_index] = left * cos - right * sin;
                tensor.values[right_index] = left * sin + right * cos;
            }
        }
    }
    Ok(())
}

fn causal_attention(
    q: &F32Tensor,
    k: &F32Tensor,
    v: &F32Tensor,
    n_heads: usize,
    n_kv_heads: usize,
    head_dim: usize,
) -> Result<F32Tensor, String> {
    if q.shape.len() != 2
        || k.shape.len() != 2
        || v.shape.len() != 2
        || k.shape[1] != q.shape[1]
        || v.shape != k.shape
        || q.shape[0] as usize != n_heads * head_dim
        || k.shape[0] as usize != n_kv_heads * head_dim
    {
        return Err("QKV Transformer incompatibles".into());
    }
    let hidden = q.shape[0] as usize;
    let tokens = q.shape[1] as usize;
    let group = n_heads / n_kv_heads;
    let mut output = vec![0.0; hidden * tokens];
    for head in 0..n_heads {
        let kv_head = head / group;
        for query in 0..tokens {
            let mut scores = vec![f32::NEG_INFINITY; query + 1];
            for (key_pos, score) in scores.iter_mut().enumerate() {
                let mut dot = 0.0;
                for d in 0..head_dim {
                    dot += q.values[(head * head_dim + d) * tokens + query]
                        * k.values[(kv_head * head_dim + d) * tokens + key_pos];
                }
                *score = dot / (head_dim as f32).sqrt();
            }
            let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mut denominator = 0.0;
            for score in &mut scores {
                *score = (*score - max).exp();
                denominator += *score;
            }
            for d in 0..head_dim {
                let value = scores
                    .iter()
                    .enumerate()
                    .map(|(key_pos, score)| {
                        score * v.values[(kv_head * head_dim + d) * tokens + key_pos]
                    })
                    .sum::<f32>()
                    / denominator;
                output[(head * head_dim + d) * tokens + query] = value;
            }
        }
    }
    F32Tensor::new(vec![hidden as u32, tokens as u32], output)
}

fn decode_attention(
    q: &F32Tensor,
    keys: &F32Tensor,
    values: &F32Tensor,
    n_heads: usize,
    n_kv_heads: usize,
    head_dim: usize,
) -> Result<F32Tensor, String> {
    if q.shape.len() != 2
        || q.shape[1] != 1
        || keys.shape != values.shape
        || keys.shape[0] as usize != n_kv_heads * head_dim
        || q.shape[0] as usize != n_heads * head_dim
        || keys.shape[1] == 0
    {
        return Err("cache QKV du décodage incompatible".into());
    }
    let hidden = q.shape[0] as usize;
    let tokens = keys.shape[1] as usize;
    let group = n_heads / n_kv_heads;
    let mut output = vec![0.0; hidden];
    for head in 0..n_heads {
        let kv_head = head / group;
        let mut scores = Vec::with_capacity(tokens);
        for key_pos in 0..tokens {
            let mut dot = 0.0;
            for d in 0..head_dim {
                dot += q.values[head * head_dim + d]
                    * keys.values[(kv_head * head_dim + d) * tokens + key_pos];
            }
            scores.push(dot / (head_dim as f32).sqrt());
        }
        let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut denominator = 0.0;
        for score in &mut scores {
            *score = (*score - max).exp();
            denominator += *score;
        }
        for d in 0..head_dim {
            output[head * head_dim + d] = scores
                .iter()
                .enumerate()
                .map(|(key_pos, score)| {
                    score * values.values[(kv_head * head_dim + d) * tokens + key_pos]
                })
                .sum::<f32>()
                / denominator;
        }
    }
    F32Tensor::new(vec![hidden as u32, 1], output)
}

fn silu(value: f32) -> f32 {
    value / (1.0 + (-value).exp())
}

impl CpuLayerExecutor {
    /// Référence déterministe : X [in, tokens] → W [out, in] × X.
    pub fn matmul(&self, input: &F32Tensor) -> Result<F32Tensor, String> {
        if self.weights.shape.len() != 2 || input.shape.len() != 2 {
            return Err("matmul Akasha attend deux matrices".into());
        }
        let (out, inner) = (
            self.weights.shape[0] as usize,
            self.weights.shape[1] as usize,
        );
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

    #[test]
    fn bloc_transformer_cpu_garde_les_dimensions_et_les_residus() {
        let vector = |shape: Vec<u32>, value: f32| F32Tensor::new(shape, vec![value; 4]).unwrap();
        let identity = || F32Tensor::new(vec![2, 2], vec![1.0, 0.0, 0.0, 1.0]).unwrap();
        let block = CpuTransformerBlock {
            norm_attn: F32Tensor::new(vec![2], vec![1.0, 1.0]).unwrap(),
            q_proj: identity(),
            k_proj: identity(),
            v_proj: identity(),
            o_proj: identity(),
            norm_ffn: F32Tensor::new(vec![2], vec![1.0, 1.0]).unwrap(),
            gate_proj: identity(),
            up_proj: identity(),
            down_proj: identity(),
            n_heads: 1,
            n_kv_heads: 1,
            eps: 1e-5,
        };
        let input = F32Tensor::new(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let output = block.forward(&input).unwrap();
        assert_eq!(output.shape, input.shape);
        assert!(output.values.iter().all(|value| value.is_finite()));
        let _ = vector;
    }

    #[test]
    fn bloc_transformer_cpu_supporte_gqa() {
        let input = F32Tensor::new(vec![4, 2], vec![1.0; 8]).unwrap();
        let identity = F32Tensor::new(
            vec![4, 4],
            vec![
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        )
        .unwrap();
        let kv = F32Tensor::new(vec![2, 4], vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0]).unwrap();
        let ffn = F32Tensor::new(
            vec![4, 4],
            vec![
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        )
        .unwrap();
        let block = CpuTransformerBlock {
            norm_attn: F32Tensor::new(vec![4], vec![1.0; 4]).unwrap(),
            q_proj: identity.clone(),
            k_proj: kv.clone(),
            v_proj: kv,
            o_proj: identity,
            norm_ffn: F32Tensor::new(vec![4], vec![1.0; 4]).unwrap(),
            gate_proj: ffn.clone(),
            up_proj: ffn.clone(),
            down_proj: ffn,
            n_heads: 2,
            n_kv_heads: 1,
            eps: 1e-5,
        };
        let output = block.forward(&input).unwrap();
        assert_eq!(output.shape, input.shape);
        assert!(output.values.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn rope_applique_la_position_aux_paires_qk() {
        let mut tensor = F32Tensor::new(vec![2, 1], vec![1.0, 0.0]).unwrap();
        apply_rope(&mut tensor, 1, 2, &[1], 100.0).unwrap();
        assert!((tensor.values[0] - 1.0f32.cos()).abs() < 1e-6);
        assert!((tensor.values[1] - 1.0f32.sin()).abs() < 1e-6);
    }

    #[test]
    fn decode_reutilise_le_cache_kv_entre_tokens() {
        let identity = F32Tensor::new(
            vec![2, 2],
            vec![1.0, 0.0, 0.0, 1.0],
        )
        .unwrap();
        let block = CpuTransformerBlock {
            norm_attn: F32Tensor::new(vec![2], vec![1.0, 1.0]).unwrap(),
            q_proj: identity.clone(),
            k_proj: identity.clone(),
            v_proj: identity.clone(),
            o_proj: identity.clone(),
            norm_ffn: F32Tensor::new(vec![2], vec![1.0, 1.0]).unwrap(),
            gate_proj: identity.clone(),
            up_proj: identity.clone(),
            down_proj: identity,
            n_heads: 1,
            n_kv_heads: 1,
            eps: 1e-5,
        };
        let mut cache = CpuKvCache::new(1, 2, 8).unwrap();
        let token = F32Tensor::new(vec![2, 1], vec![1.0, 2.0]).unwrap();
        assert!(block.decode_with_cache(&token, 0, &mut cache, Some(100.0)).is_ok());
        assert!(block.decode_with_cache(&token, 1, &mut cache, Some(100.0)).is_ok());
        assert_eq!(cache.len(), 2);

        let prompt = F32Tensor::new(vec![2, 2], vec![1.0, 2.0, 2.0, 1.0]).unwrap();
        let mut prefill_cache = CpuKvCache::new(1, 2, 8).unwrap();
        let prefilled = block
            .prefill_with_cache(&prompt, 0, &mut prefill_cache, Some(100.0))
            .unwrap();
        assert_eq!(prefilled.shape, vec![2, 2]);
        assert_eq!(prefill_cache.len(), 2);
    }
}
