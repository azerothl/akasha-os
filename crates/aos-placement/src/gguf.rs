//! Lecteur GGUF minimal indépendant de llama.cpp.
//!
//! Il valide l'en-tête, les métadonnées et l'index des tenseurs. La lecture de
//! données est volontairement limitée à F32 pour fournir une première
//! référence sûre à l'adaptateur RPC ; les blocs quantifiés nécessitent un
//! décodeur par format et sont refusés explicitement.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use memmap2::Mmap;

const MAGIC: &[u8; 4] = b"GGUF";
const DEFAULT_ALIGNMENT: u64 = 32;
const MAX_STRING: usize = 1 << 20;
const MAX_TENSORS: u64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GgufTensorType {
    F32,
    Q4_0,
    Q4_1,
    Q5_0,
    Q5_1,
    Q8_0,
    Q2K,
    Q3K,
    Q4K,
    Q5K,
    Q6K,
    Other(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgufTensorInfo {
    pub name: String,
    pub dimensions: Vec<u64>,
    pub tensor_type: GgufTensorType,
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GgufMetadataValue {
    U32(u32),
    U64(u64),
    F32(f32),
    Other,
}

#[derive(Debug)]
enum GgufBacking {
    Owned(Vec<u8>),
    Mapped(Mmap),
}

impl GgufBacking {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(bytes) => bytes,
            Self::Mapped(bytes) => bytes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GgufModel {
    bytes: Arc<GgufBacking>,
    data_start: u64,
    pub version: u32,
    pub alignment: u64,
    pub metadata: BTreeMap<String, GgufMetadataValue>,
    pub tensors: Vec<GgufTensorInfo>,
}

impl GgufModel {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("lecture GGUF impossible: {e}"))?;
        let backing =
            unsafe { Mmap::map(&file).map_err(|e| format!("mapping GGUF impossible: {e}"))? };
        Self::from_backing(GgufBacking::Mapped(backing))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        Self::from_backing(GgufBacking::Owned(bytes))
    }

    fn from_backing(backing: GgufBacking) -> Result<Self, String> {
        let backing = Arc::new(backing);
        let bytes = backing.as_slice();
        let mut reader = Reader { bytes, pos: 0 };
        if reader.read_bytes(4)? != MAGIC {
            return Err("magic GGUF invalide".into());
        }
        let version = reader.u32()?;
        if !(2..=3).contains(&version) {
            return Err(format!("version GGUF non supportée: {version}"));
        }
        let tensor_count = reader.u64()?;
        let kv_count = reader.u64()?;
        if tensor_count > MAX_TENSORS {
            return Err("GGUF contient trop de tenseurs".into());
        }
        let mut alignment = DEFAULT_ALIGNMENT;
        let mut metadata = BTreeMap::new();
        for _ in 0..kv_count {
            let key = reader.string()?;
            let value = reader.metadata_value()?;
            if key == "general.alignment" {
                if let GgufMetadataValue::U32(value) = value {
                    alignment = u64::from(value);
                }
            }
            metadata.insert(key, value);
        }
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err("alignement GGUF invalide".into());
        }
        let mut tensors = Vec::with_capacity(tensor_count as usize);
        for _ in 0..tensor_count {
            let name = reader.string()?;
            let dimensions_count = reader.u32()? as usize;
            if dimensions_count == 0 || dimensions_count > 4 {
                return Err("rang de tenseur GGUF invalide".into());
            }
            let mut dimensions = Vec::with_capacity(dimensions_count);
            for _ in 0..dimensions_count {
                let dimension = reader.u64()?;
                if dimension == 0 {
                    return Err("dimension GGUF nulle".into());
                }
                dimensions.push(dimension);
            }
            let tensor_type = match reader.u32()? {
                0 => GgufTensorType::F32,
                2 => GgufTensorType::Q4_0,
                3 => GgufTensorType::Q4_1,
                6 => GgufTensorType::Q5_0,
                7 => GgufTensorType::Q5_1,
                8 => GgufTensorType::Q8_0,
                10 => GgufTensorType::Q2K,
                11 => GgufTensorType::Q3K,
                12 => GgufTensorType::Q4K,
                13 => GgufTensorType::Q5K,
                14 => GgufTensorType::Q6K,
                other => GgufTensorType::Other(other),
            };
            let offset = reader.u64()?;
            tensors.push(GgufTensorInfo {
                name,
                dimensions,
                tensor_type,
                offset,
            });
        }
        let data_start = align_up(reader.pos as u64, alignment)?;
        let data_len = bytes
            .len()
            .checked_sub(data_start as usize)
            .ok_or("section de données GGUF absente")?;
        for tensor in &tensors {
            let size = tensor_size(tensor)?;
            let end = tensor
                .offset
                .checked_add(size)
                .ok_or("offset de tenseur GGUF débordant")?;
            if end > data_len as u64 {
                return Err(format!("tenseur GGUF hors fichier: {}", tensor.name));
            }
        }
        Ok(Self {
            bytes: backing,
            data_start,
            version,
            alignment,
            metadata,
            tensors,
        })
    }

    pub fn metadata_u32(&self, key: &str) -> Option<u32> {
        match self.metadata.get(key) {
            Some(GgufMetadataValue::U32(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn metadata_u64(&self, key: &str) -> Option<u64> {
        match self.metadata.get(key) {
            Some(GgufMetadataValue::U64(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn metadata_f32(&self, key: &str) -> Option<f32> {
        match self.metadata.get(key) {
            Some(GgufMetadataValue::F32(value)) => Some(*value),
            _ => None,
        }
    }

    /// Absolute byte offset at which GGUF tensor storage starts.
    pub fn data_start(&self) -> u64 {
        self.data_start
    }

    /// Logical size of the source GGUF file.
    pub fn file_len(&self) -> u64 {
        self.bytes.as_slice().len() as u64
    }

    /// Return the absolute byte range occupied by one tensor in the source
    /// file. This is used by the LAN shard planner; it intentionally exposes
    /// no mutable view of the model bytes.
    pub fn tensor_data_range(&self, name: &str) -> Result<(u64, u64), String> {
        let tensor = self
            .tensors
            .iter()
            .find(|tensor| tensor.name == name)
            .ok_or_else(|| format!("tenseur GGUF absent: {name}"))?;
        let size = tensor_size(tensor)?;
        let start = self
            .data_start
            .checked_add(tensor.offset)
            .ok_or_else(|| format!("offset de tenseur GGUF débordant: {name}"))?;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= self.file_len())
            .ok_or_else(|| format!("tenseur GGUF hors fichier: {name}"))?;
        Ok((start, end - start))
    }

    /// Return the GGUF header/index range plus all tensor ranges needed by a
    /// contiguous layer segment. Ranges are sorted and never overlap. The
    /// header is always included because workers need model metadata to decode
    /// a layer independently.
    pub fn layer_data_ranges(
        &self,
        first_layer: u32,
        last_layer: u32,
    ) -> Result<Vec<(u64, u64)>, String> {
        if first_layer > last_layer {
            return Err("segment GGUF vide".into());
        }
        let mut ranges = vec![(0, self.data_start)];
        for layer in first_layer..=last_layer {
            let prefix = format!("blk.{layer}.");
            for tensor in self
                .tensors
                .iter()
                .filter(|tensor| tensor.name.starts_with(&prefix))
            {
                ranges.push(self.tensor_data_range(&tensor.name)?);
            }
        }
        ranges.sort_by_key(|(offset, _)| *offset);
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
        for (offset, length) in ranges {
            if length == 0 {
                continue;
            }
            if let Some((previous_offset, previous_length)) = merged.last_mut() {
                let previous_end = previous_offset.saturating_add(*previous_length);
                if offset <= previous_end {
                    let end = offset.saturating_add(length);
                    *previous_length = (*previous_length).max(end.saturating_sub(*previous_offset));
                    continue;
                }
            }
            merged.push((offset, length));
        }
        Ok(merged)
    }

    pub fn tensor_f32_matrix(&self, name: &str) -> Result<crate::tensor_wire::F32Tensor, String> {
        let info = self
            .tensors
            .iter()
            .find(|tensor| tensor.name == name)
            .ok_or_else(|| format!("tenseur GGUF absent: {name}"))?;
        let shape = info
            .dimensions
            .iter()
            .map(|&dimension| {
                u32::try_from(dimension).map_err(|_| format!("dimension GGUF trop grande: {name}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        crate::tensor_wire::F32Tensor::new(shape, self.tensor_f32(name)?)
    }

    /// Charge une matrice GGML dans l'ordre de calcul Akasha [sortie, entrée].
    /// GGUF conserve la première dimension contiguë, donc une matrice GGML
    /// [entrée, sortie] doit être transposée avant `W × X`.
    pub fn tensor_f32_weight(&self, name: &str) -> Result<crate::tensor_wire::F32Tensor, String> {
        let info = self
            .tensors
            .iter()
            .find(|tensor| tensor.name == name)
            .ok_or_else(|| format!("tenseur GGUF absent: {name}"))?;
        if info.dimensions.len() != 2 {
            return Err(format!("poids GGUF non matriciel: {name}"));
        }
        let input = usize::try_from(info.dimensions[0])
            .map_err(|_| format!("dimension GGUF trop grande: {name}"))?;
        let output = usize::try_from(info.dimensions[1])
            .map_err(|_| format!("dimension GGUF trop grande: {name}"))?;
        let values = self.tensor_f32(name)?;
        if values.len() != input.saturating_mul(output) {
            return Err(format!("taille de poids GGUF incohérente: {name}"));
        }
        let mut transposed = Vec::with_capacity(values.len());
        for out in 0..output {
            for inner in 0..input {
                transposed.push(values[inner + out * input]);
            }
        }
        crate::tensor_wire::F32Tensor::new(vec![output as u32, input as u32], transposed)
    }

    /// Lit une ligne de poids GGML [entrée, sortie] et la retourne dans
    /// l'ordre de calcul [sortie, entrée]. Cette variante évite de décoder
    /// toute la matrice de sortie (souvent plusieurs centaines de MiB) pour
    /// calculer les logits d'un seul token.
    pub fn tensor_f32_weight_row(
        &self,
        name: &str,
        output_index: usize,
    ) -> Result<Vec<f32>, String> {
        let info = self
            .tensors
            .iter()
            .find(|tensor| tensor.name == name)
            .ok_or_else(|| format!("tenseur GGUF absent: {name}"))?;
        if info.dimensions.len() != 2 {
            return Err(format!("poids GGUF non matriciel: {name}"));
        }
        let input = usize::try_from(info.dimensions[0])
            .map_err(|_| format!("dimension GGUF trop grande: {name}"))?;
        let output = usize::try_from(info.dimensions[1])
            .map_err(|_| format!("dimension GGUF trop grande: {name}"))?;
        if output_index >= output {
            return Err(format!("ligne GGUF hors limites: {name}"));
        }
        let total_size = tensor_size(info)? as usize;
        let row_size = total_size
            .checked_div(output)
            .filter(|size| *size > 0)
            .ok_or_else(|| format!("taille de ligne GGUF invalide: {name}"))?;
        let start = self
            .data_start
            .checked_add(info.offset)
            .and_then(|value| value.checked_add((output_index * row_size) as u64))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| format!("offset de ligne GGUF débordant: {name}"))?;
        let end = start
            .checked_add(row_size)
            .ok_or_else(|| format!("ligne GGUF débordante: {name}"))?;
        let bytes = self
            .bytes
            .as_slice()
            .get(start..end)
            .ok_or("ligne GGUF tronquée")?;
        let values = match info.tensor_type {
            GgufTensorType::F32 => bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| f32::from_le_bytes(*chunk))
                .collect(),
            GgufTensorType::Q4_0 => decode_q4_0(bytes, input)?,
            GgufTensorType::Q4_1 => decode_q4_1(bytes, input)?,
            GgufTensorType::Q5_0 => decode_q5_0(bytes, input)?,
            GgufTensorType::Q5_1 => decode_q5_1(bytes, input)?,
            GgufTensorType::Q8_0 => decode_q8_0(bytes, input)?,
            GgufTensorType::Q2K => decode_q2_k(bytes, input)?,
            GgufTensorType::Q3K => decode_q3_k(bytes, input)?,
            GgufTensorType::Q4K => decode_q4_k(bytes, input)?,
            GgufTensorType::Q5K => decode_q5_k(bytes, input)?,
            GgufTensorType::Q6K => decode_q6_k(bytes, input)?,
            GgufTensorType::Other(kind) => return Err(format!("type GGUF non supporté: {kind}")),
        };
        if values.len() != input || values.iter().any(|value| !value.is_finite()) {
            return Err(format!("ligne GGUF non finie ou incohérente: {name}"));
        }
        Ok(values)
    }

    pub fn tensor_f32(&self, name: &str) -> Result<Vec<f32>, String> {
        let info = self
            .tensors
            .iter()
            .find(|tensor| tensor.name == name)
            .ok_or_else(|| format!("tenseur GGUF absent: {name}"))?;
        let size = tensor_size(info)? as usize;
        let start = self.data_start as usize + info.offset as usize;
        match info.tensor_type {
            GgufTensorType::F32 => Ok(self.bytes.as_slice()[start..start + size]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| f32::from_le_bytes(*bytes))
                .collect()),
            GgufTensorType::Q4_0 => decode_q4_0(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q4_1 => decode_q4_1(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q5_0 => decode_q5_0(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q5_1 => decode_q5_1(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q8_0 => decode_q8_0(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q2K => decode_q2_k(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q3K => decode_q3_k(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q4K => decode_q4_k(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q5K => decode_q5_k(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Q6K => decode_q6_k(
                &self.bytes.as_slice()[start..start + size],
                info.dimensions.iter().product::<u64>() as usize,
            ),
            GgufTensorType::Other(_) => Err(format!(
                "tenseur GGUF {} non supporté: {:?}",
                name, info.tensor_type
            )),
        }
    }
}

fn tensor_size(info: &GgufTensorInfo) -> Result<u64, String> {
    let elements = info
        .dimensions
        .iter()
        .try_fold(1u64, |size, dimension| size.checked_mul(*dimension))
        .ok_or_else(|| "taille de tenseur GGUF débordante".to_string())?;
    match info.tensor_type {
        GgufTensorType::F32 => elements
            .checked_mul(4)
            .ok_or_else(|| "taille de tenseur GGUF débordante".into()),
        GgufTensorType::Q4_0 => {
            if elements % 32 != 0 {
                return Err("tenseur Q4_0 non multiple de 32".into());
            }
            (elements / 32)
                .checked_mul(18)
                .ok_or_else(|| "taille de tenseur GGUF débordante".into())
        }
        GgufTensorType::Q4_1 => block_size(elements, 20, "Q4_1"),
        GgufTensorType::Q5_0 => block_size(elements, 22, "Q5_0"),
        GgufTensorType::Q5_1 => block_size(elements, 24, "Q5_1"),
        GgufTensorType::Q8_0 => block_size(elements, 34, "Q8_0"),
        GgufTensorType::Q2K => block_size_k(elements, 132, "Q2_K"),
        GgufTensorType::Q3K => block_size_k(elements, 110, "Q3_K"),
        GgufTensorType::Q4K => block_size_k(elements, 144, "Q4_K"),
        GgufTensorType::Q5K => block_size_k(elements, 176, "Q5_K"),
        GgufTensorType::Q6K => block_size_k(elements, 210, "Q6_K"),
        GgufTensorType::Other(kind) => Err(format!("type GGUF non supporté: {kind}")),
    }
}

fn block_size(elements: u64, bytes_per_block: u64, name: &str) -> Result<u64, String> {
    if !elements.is_multiple_of(32) {
        return Err(format!("tenseur {name} non multiple de 32"));
    }
    (elements / 32)
        .checked_mul(bytes_per_block)
        .ok_or_else(|| "taille de tenseur GGUF débordante".into())
}

fn block_size_k(elements: u64, bytes_per_block: u64, name: &str) -> Result<u64, String> {
    if !elements.is_multiple_of(256) {
        return Err(format!("tenseur {name} non multiple de 256"));
    }
    (elements / 256)
        .checked_mul(bytes_per_block)
        .ok_or_else(|| "taille de tenseur GGUF débordante".into())
}

fn decode_q4_0(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    if !elements.is_multiple_of(32) || bytes.len() != elements / 32 * 18 {
        return Err("bloc Q4_0 GGUF invalide".into());
    }
    let mut values = Vec::with_capacity(elements);
    for block in bytes.as_chunks::<18>().0 {
        let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
        for byte in &block[2..18] {
            values.push(scale * ((byte & 0x0f) as f32 - 8.0));
        }
        for byte in &block[2..18] {
            values.push(scale * ((byte >> 4) as f32 - 8.0));
        }
    }
    Ok(values)
}

fn decode_q4_1(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks(bytes, elements, 20, |block, values| {
        let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
        let minimum = f16_to_f32(u16::from_le_bytes([block[2], block[3]]));
        for byte in &block[4..20] {
            values.push(scale * (byte & 0x0f) as f32 + minimum);
        }
        for byte in &block[4..20] {
            values.push(scale * (byte >> 4) as f32 + minimum);
        }
    })
}

fn decode_q5_0(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks(bytes, elements, 22, |block, values| {
        let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
        let high = u32::from_le_bytes(block[2..6].try_into().unwrap());
        for (index, byte) in block[6..22].iter().enumerate() {
            let high_bit = (((high >> index) & 1) << 4) as u8;
            values.push(scale * (((byte & 0x0f) | high_bit) as f32 - 16.0));
        }
        for (index, byte) in block[6..22].iter().enumerate() {
            let high_bit = (((high >> (index + 16)) & 1) << 4) as u8;
            values.push(scale * (((byte >> 4) | high_bit) as f32 - 16.0));
        }
    })
}

fn decode_q5_1(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks(bytes, elements, 24, |block, values| {
        let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
        let minimum = f16_to_f32(u16::from_le_bytes([block[2], block[3]]));
        let high = u32::from_le_bytes(block[4..8].try_into().unwrap());
        for (index, byte) in block[8..24].iter().enumerate() {
            let high_bit = (((high >> index) & 1) << 4) as u8;
            values.push(scale * ((byte & 0x0f) | high_bit) as f32 + minimum);
        }
        for (index, byte) in block[8..24].iter().enumerate() {
            let high_bit = (((high >> (index + 16)) & 1) << 4) as u8;
            values.push(scale * ((byte >> 4) | high_bit) as f32 + minimum);
        }
    })
}

fn decode_q8_0(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks(bytes, elements, 34, |block, values| {
        let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
        values.extend(
            block[2..34]
                .iter()
                .map(|value| scale * (*value as i8) as f32),
        );
    })
}

fn decode_q2_k(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks_k(bytes, elements, 132, |block, values| {
        let d = f16_to_f32(u16::from_le_bytes([block[128], block[129]]));
        let minimum_scale = f16_to_f32(u16::from_le_bytes([block[130], block[131]]));
        let scales = &block[0..64];
        let quants = &block[64..128];
        for half in 0..2 {
            let quant_start = half * 32;
            for group in 0..4 {
                let scale = scales[half * 8 + group * 2];
                let d_group = d * (scale & 0x0f) as f32;
                let m_group = minimum_scale * (scale >> 4) as f32;
                let shift = group * 2;
                for index in 0..16 {
                    values.push(
                        d_group * ((quants[quant_start + index] >> shift) & 3) as f32 - m_group,
                    );
                }
                let scale = scales[half * 8 + group * 2 + 1];
                let d_group = d * (scale & 0x0f) as f32;
                let m_group = minimum_scale * (scale >> 4) as f32;
                for index in 0..16 {
                    values.push(
                        d_group * ((quants[quant_start + 16 + index] >> shift) & 3) as f32
                            - m_group,
                    );
                }
            }
        }
    })
}

fn decode_q3_k(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks_k(bytes, elements, 110, |block, values| {
        let high_mask = &block[0..32];
        let quants = &block[32..96];
        let raw_scales = &block[96..108];
        let d = f16_to_f32(u16::from_le_bytes([block[108], block[109]]));
        let mut aux = [0u32; 4];
        for (index, value) in aux.iter_mut().enumerate() {
            *value = u32::from_le_bytes(raw_scales[index * 4..index * 4 + 4].try_into().unwrap());
        }
        let tmp = aux[2];
        aux[2] = ((aux[0] >> 4) & 0x0f0f0f0f) | (((tmp >> 4) & 0x03030303) << 4);
        aux[3] = ((aux[1] >> 4) & 0x0f0f0f0f) | (((tmp >> 6) & 0x03030303) << 4);
        aux[0] = (aux[0] & 0x0f0f0f0f) | ((tmp & 0x03030303) << 4);
        aux[1] = (aux[1] & 0x0f0f0f0f) | (((tmp >> 2) & 0x03030303) << 4);
        let scale_bytes: Vec<i8> = aux
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .map(|value| value as i8)
            .collect();
        for half in 0..2 {
            for group in 0..4 {
                let scale = d * (i32::from(scale_bytes[half * 8 + group * 2]) - 32) as f32;
                let shift = group * 2;
                let mask = 1u8 << (half * 4 + group);
                let quant_start = half * 32;
                for index in 0..16 {
                    let low = (quants[quant_start + index] >> shift) & 3;
                    let high = if high_mask[quant_start + index] & mask != 0 {
                        0
                    } else {
                        4
                    };
                    values.push(scale * (low as i8 - high) as f32);
                }
                let scale = d * (i32::from(scale_bytes[half * 8 + group * 2 + 1]) - 32) as f32;
                for index in 0..16 {
                    let low = (quants[quant_start + 16 + index] >> shift) & 3;
                    let high = if high_mask[quant_start + 16 + index] & mask != 0 {
                        0
                    } else {
                        4
                    };
                    values.push(scale * (low as i8 - high) as f32);
                }
            }
        }
    })
}

fn scale_min_k4(index: usize, scales: &[u8]) -> (u8, u8) {
    if index < 4 {
        (scales[index] & 63, scales[index + 4] & 63)
    } else {
        (
            (scales[index + 4] & 0x0f) | ((scales[index - 4] >> 6) << 4),
            (scales[index + 4] >> 4) | ((scales[index] >> 6) << 4),
        )
    }
}

fn decode_q4_k(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks_k(bytes, elements, 144, |block, values| {
        let d = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
        let minimum_scale = f16_to_f32(u16::from_le_bytes([block[2], block[3]]));
        let scales = &block[4..16];
        let quants = &block[16..144];
        for group in 0..8 {
            let (scale, minimum) = scale_min_k4(group, scales);
            let d_group = d * scale as f32;
            let m_group = minimum_scale * minimum as f32;
            let start = group * 32;
            for index in 0..32 {
                let quant = if index < 16 {
                    quants[start / 2 + index] & 0x0f
                } else {
                    quants[start / 2 + index - 16] >> 4
                };
                values.push(d_group * quant as f32 - m_group);
            }
        }
    })
}

fn decode_q5_k(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks_k(bytes, elements, 176, |block, values| {
        let d = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
        let minimum_scale = f16_to_f32(u16::from_le_bytes([block[2], block[3]]));
        let scales = &block[4..16];
        let high = &block[16..48];
        let quants = &block[48..176];
        for group in 0..8 {
            let (scale, minimum) = scale_min_k4(group, scales);
            let d_group = d * scale as f32;
            let m_group = minimum_scale * minimum as f32;
            let start = group * 32;
            for index in 0..32 {
                let low = if index < 16 {
                    quants[start / 2 + index] & 0x0f
                } else {
                    quants[start / 2 + index - 16] >> 4
                };
                let high_bit = (high[index % 32] >> group) & 1;
                values.push(d_group * (low as f32 + high_bit as f32 * 16.0) - m_group);
            }
        }
    })
}

fn decode_q6_k(bytes: &[u8], elements: usize) -> Result<Vec<f32>, String> {
    decode_blocks_k(bytes, elements, 210, |block, values| {
        let quants_low = &block[0..128];
        let quants_high = &block[128..192];
        let scales = &block[192..208];
        let d = f16_to_f32(u16::from_le_bytes([block[208], block[209]]));
        for chunk in 0..2 {
            for index in 0..128 {
                let low_byte = quants_low[chunk * 64 + index % 64];
                let high_byte = quants_high[chunk * 32 + index % 32];
                let low = if index < 64 {
                    low_byte & 0x0f
                } else {
                    low_byte >> 4
                };
                let high = (high_byte >> ((index / 32) % 4 * 2)) & 3;
                let scale = scales[(chunk * 8) + index / 16] as i8 as f32;
                values.push(d * scale * ((low | (high << 4)) as i8 - 32) as f32);
            }
        }
    })
}

fn decode_blocks_k(
    bytes: &[u8],
    elements: usize,
    bytes_per_block: usize,
    decode: impl Fn(&[u8], &mut Vec<f32>),
) -> Result<Vec<f32>, String> {
    if !elements.is_multiple_of(256) || bytes.len() != elements / 256 * bytes_per_block {
        return Err("bloc K GGUF invalide".into());
    }
    let mut values = Vec::with_capacity(elements);
    for block in bytes.chunks_exact(bytes_per_block) {
        decode(block, &mut values);
    }
    Ok(values)
}

fn decode_blocks(
    bytes: &[u8],
    elements: usize,
    bytes_per_block: usize,
    decode: impl Fn(&[u8], &mut Vec<f32>),
) -> Result<Vec<f32>, String> {
    if !elements.is_multiple_of(32) || bytes.len() != elements / 32 * bytes_per_block {
        return Err("bloc GGUF invalide".into());
    }
    let mut values = Vec::with_capacity(elements);
    for block in bytes.chunks_exact(bytes_per_block) {
        decode(block, &mut values);
    }
    Ok(values)
}

fn f16_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) as u32) << 31;
    let exponent = (bits >> 10) & 0x1f;
    let fraction = (bits & 0x03ff) as u32;
    let raw = match exponent {
        0 => {
            if fraction == 0 {
                sign
            } else {
                let mut fraction = fraction;
                let mut exponent = 0i32;
                while fraction & 0x0400 == 0 {
                    fraction <<= 1;
                    exponent -= 1;
                }
                let mantissa = fraction & 0x03ff;
                sign | (((exponent + 127 + 15) as u32) << 23) | (mantissa << 13)
            }
        }
        0x1f => sign | 0x7f800000 | (fraction << 13),
        exponent => sign | (((exponent as u32 - 15 + 127) << 23) | (fraction << 13)),
    };
    f32::from_bits(raw)
}

fn align_up(value: u64, alignment: u64) -> Result<u64, String> {
    let remainder = value % alignment;
    if remainder == 0 {
        Ok(value)
    } else {
        value
            .checked_add(alignment - remainder)
            .ok_or_else(|| "alignement GGUF débordant".into())
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(count).ok_or("GGUF tronqué")?;
        let value = self.bytes.get(self.pos..end).ok_or("GGUF tronqué")?;
        self.pos = end;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.read_bytes(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.read_bytes(8)?.try_into().unwrap()))
    }

    fn string(&mut self) -> Result<String, String> {
        let length = self.u64()? as usize;
        if length > MAX_STRING {
            return Err("chaîne GGUF trop longue".into());
        }
        String::from_utf8(self.read_bytes(length)?.to_vec())
            .map_err(|_| "chaîne GGUF non UTF-8".into())
    }

    fn metadata_value(&mut self) -> Result<GgufMetadataValue, String> {
        let kind = self.u32()?;
        match kind {
            4 => Ok(GgufMetadataValue::U32(self.u32()?)),
            8 => {
                let length = self.u64()? as usize;
                if length > MAX_STRING {
                    return Err("métadonnée GGUF trop longue".into());
                }
                let _ = self.read_bytes(length)?;
                Ok(GgufMetadataValue::Other)
            }
            9 => {
                let element_kind = self.u32()?;
                if element_kind == 9 {
                    return Err("tableaux GGUF imbriqués non supportés".into());
                }
                let length = self.u64()?;
                if length > MAX_TENSORS {
                    return Err("tableau de métadonnées GGUF trop long".into());
                }
                for _ in 0..length {
                    self.metadata_scalar(element_kind)?;
                }
                Ok(GgufMetadataValue::Other)
            }
            0 | 1 | 2 | 3 | 5 | 6 | 7 | 10 | 11 | 12 => {
                let width = match kind {
                    0 | 1 | 7 => 1,
                    2 | 3 => 2,
                    4..=6 => 4,
                    10..=12 => 8,
                    _ => unreachable!(),
                };
                let bytes = self.read_bytes(width)?;
                match kind {
                    6 => Ok(GgufMetadataValue::F32(f32::from_le_bytes(
                        bytes.try_into().unwrap(),
                    ))),
                    10 => Ok(GgufMetadataValue::U64(u64::from_le_bytes(
                        bytes.try_into().unwrap(),
                    ))),
                    _ => Ok(GgufMetadataValue::Other),
                }
            }
            _ => Err(format!("type de métadonnée GGUF inconnu: {kind}")),
        }
    }

    fn metadata_scalar(&mut self, kind: u32) -> Result<(), String> {
        let width = match kind {
            0 | 1 | 7 => 1,
            2 | 3 => 2,
            4..=6 => 4,
            10..=12 => 8,
            8 => {
                let length = self.u64()? as usize;
                if length > MAX_STRING {
                    return Err("chaîne GGUF trop longue".into());
                }
                let _ = self.read_bytes(length)?;
                return Ok(());
            }
            _ => return Err(format!("type de tableau GGUF inconnu: {kind}")),
        };
        let _ = self.read_bytes(width)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&(value.len() as u64).to_le_bytes());
        out.extend_from_slice(value.as_bytes());
    }

    #[test]
    fn lit_index_et_tenseur_f32() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        string(&mut bytes, "x");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&2u64.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        while bytes.len() % 32 != 0 {
            bytes.push(0);
        }
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        bytes.extend_from_slice(&2.5f32.to_le_bytes());
        let path = std::env::temp_dir().join(format!("akasha-gguf-mmap-{}", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let mapped = GgufModel::open(&path).unwrap();
        assert_eq!(mapped.tensor_f32("x").unwrap(), vec![1.5, 2.5]);
        std::fs::remove_file(&path).unwrap();
        let model = GgufModel::from_bytes(bytes).unwrap();
        assert_eq!(model.tensor_f32("x").unwrap(), vec![1.5, 2.5]);
        assert!(model.data_start() >= 32);
        assert_eq!(
            model.tensor_data_range("x").unwrap(),
            (model.data_start(), 8)
        );
        assert_eq!(
            model.layer_data_ranges(0, 0).unwrap(),
            vec![(0, model.data_start())]
        );
    }

    #[test]
    fn transpose_les_poids_ggml_vers_l_ordre_de_calcul() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        string(&mut bytes, "w");
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&2u64.to_le_bytes());
        bytes.extend_from_slice(&3u64.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        while bytes.len() % 32 != 0 {
            bytes.push(0);
        }
        for value in 1..=6 {
            bytes.extend_from_slice(&(value as f32).to_le_bytes());
        }
        let model = GgufModel::from_bytes(bytes).unwrap();
        let weight = model.tensor_f32_weight("w").unwrap();
        assert_eq!(weight.shape, vec![3, 2]);
        assert_eq!(weight.values, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(model.tensor_f32_weight_row("w", 2).unwrap(), vec![5.0, 6.0]);
    }

    #[test]
    fn refuse_type_quantifie() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        string(&mut bytes, "x");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&12u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        while bytes.len() % 32 != 0 {
            bytes.push(0);
        }
        assert!(GgufModel::from_bytes(bytes).is_err());
    }

    #[test]
    fn decode_un_bloc_q4_0() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        string(&mut bytes, "x");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&32u64.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        while bytes.len() % 32 != 0 {
            bytes.push(0);
        }
        bytes.extend_from_slice(&0x3c00u16.to_le_bytes());
        bytes.extend(std::iter::repeat_n(0x88, 16));
        let model = GgufModel::from_bytes(bytes).unwrap();
        assert_eq!(model.tensor_f32("x").unwrap(), vec![0.0; 32]);
    }

    #[test]
    fn decode_formats_bloc_simples() {
        let mut q4_1 = vec![0u8; 20];
        q4_1[0] = 0x00;
        q4_1[1] = 0x3c;
        q4_1[4..].fill(0x11);
        assert_eq!(decode_q4_1(&q4_1, 32).unwrap(), vec![1.0; 32]);

        let q5_0 = vec![0u8; 22];
        assert_eq!(decode_q5_0(&q5_0, 32).unwrap().len(), 32);

        let q5_1 = vec![0u8; 24];
        assert_eq!(decode_q5_1(&q5_1, 32).unwrap().len(), 32);

        let mut q8_0 = vec![0u8; 34];
        q8_0[0] = 0x00;
        q8_0[1] = 0x3c;
        q8_0[2..].fill(2);
        assert_eq!(decode_q8_0(&q8_0, 32).unwrap(), vec![2.0; 32]);

        assert_eq!(decode_q4_k(&[0u8; 144], 256).unwrap(), vec![0.0; 256]);
        assert_eq!(decode_q5_k(&[0u8; 176], 256).unwrap(), vec![0.0; 256]);
        assert_eq!(decode_q6_k(&[0u8; 210], 256).unwrap(), vec![0.0; 256]);
    }

    #[test]
    fn lit_un_tableau_de_metadonnees_gguf() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&1u64.to_le_bytes());
        string(&mut bytes, "test.array");
        bytes.extend_from_slice(&9u32.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&2u64.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        while bytes.len() % 32 != 0 {
            bytes.push(0);
        }
        assert!(GgufModel::from_bytes(bytes).is_ok());
    }

    #[test]
    fn conserve_les_metadonnees_numeriques_utiles_au_planner() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GGUF");
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&3u64.to_le_bytes());
        string(&mut bytes, "llama.attention.head_count");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        string(&mut bytes, "llama.attention.layer_norm_rms_epsilon");
        bytes.extend_from_slice(&6u32.to_le_bytes());
        bytes.extend_from_slice(&1e-5f32.to_le_bytes());
        string(&mut bytes, "general.quantization_version");
        bytes.extend_from_slice(&10u32.to_le_bytes());
        bytes.extend_from_slice(&2u64.to_le_bytes());
        while bytes.len() % 32 != 0 {
            bytes.push(0);
        }
        let model = GgufModel::from_bytes(bytes).unwrap();
        assert_eq!(model.metadata_u32("llama.attention.head_count"), Some(4));
        assert_eq!(model.metadata_u64("general.quantization_version"), Some(2));
        assert_eq!(
            model.metadata_f32("llama.attention.layer_norm_rms_epsilon"),
            Some(1e-5)
        );
    }
}
