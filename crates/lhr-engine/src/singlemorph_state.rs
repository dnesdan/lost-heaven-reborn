//! Original PC morphBase weights and CPU vertex summation shared by MRPH and
//! singlemorph before its inherited singlemesh skinning.
//!
//! This implements the caller-owned data portion of morphBase::LoadImageB,
//! NormalizeWeights and UpdateMorphLod. Driver buffers, allocation failure,
//! reference ownership and the inherited singlemesh update remain caller work.

use crate::native_matrix::{
    checked_f32_add as add, checked_f32_mul as mul, MatrixArithmeticUnavailable,
};
use lhr_assets::{MorphRecord, Vertex};

const DIRTY: u32 = 1;
const LOADED: u32 = 2;
const THRESHOLD: f32 = f32::from_bits(0x322b_cc77);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinglemorphError {
    Definition,
    WeightIndex,
    MissingVertexIndices,
    VertexIndex,
    NumericDomain,
}

impl std::fmt::Display for SinglemorphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "singlemorph unavailable: {self:?}")
    }
}
impl std::error::Error for SinglemorphError {}
impl From<MatrixArithmeticUnavailable> for SinglemorphError {
    fn from(_: MatrixArithmeticUnavailable) -> Self {
        Self::NumericDomain
    }
}

/// Native morph prefix with owned weights. Normalized storage is unavailable
/// until normalization; the original allocation initially leaves it unwritten.
#[derive(Clone, Debug)]
pub struct SinglemorphState {
    flags: u32,
    levels: usize,
    regions: usize,
    weights: Vec<f32>,
    normalized: Option<Vec<f32>>,
}

impl Default for SinglemorphState {
    fn default() -> Self {
        Self::new()
    }
}

impl SinglemorphState {
    /// Original morphBase constructor (RVA 0x34370).
    pub fn new() -> Self {
        Self {
            flags: 5,
            levels: 0,
            regions: 0,
            weights: Vec::new(),
            normalized: None,
        }
    }

    /// Successful parsed-data projection of morphBase::LoadImageB (0x34bd0)
    /// after the derived LoadImage clears flag4 (MRPH0x36750/SMMR0x575d0).
    /// Each region initially selects level zero. The caller owns the appropriate
    /// inherited mesh load and immutable definition lifetime.
    /// It does not reproduce stream reads, partial failure or native ownership.
    pub fn from_record(record: &MorphRecord) -> Result<Self, SinglemorphError> {
        let levels = usize::from(record.target_count);
        let mut state = Self::new();
        state.flags &= !4;
        if levels == 0 {
            if record.definition.is_some() {
                return Err(SinglemorphError::Definition);
            }
            return Ok(state);
        }
        let definition = record
            .definition
            .as_ref()
            .ok_or(SinglemorphError::Definition)?;
        let regions = usize::from(definition.channel_count);
        for lod in &definition.lods {
            if lod.channels.len() != regions {
                return Err(SinglemorphError::Definition);
            }
            for channel in &lod.channels {
                let count = usize::from(channel.vertex_count);
                if channel.samples.len() != count * levels
                    || (count == 0
                        && (channel.index_mode.is_some() || !channel.vertex_indices.is_empty()))
                    || (count != 0 && channel.index_mode.is_none())
                    || (channel.index_mode == Some(0) && !channel.vertex_indices.is_empty())
                    || (channel.index_mode.is_some_and(|mode| mode != 0)
                        && channel.vertex_indices.len() != count)
                {
                    return Err(SinglemorphError::Definition);
                }
            }
        }
        state.levels = levels;
        state.regions = regions;
        state.weights = vec![0.0; levels * regions];
        for region in state.weights.chunks_exact_mut(levels) {
            region[0] = 1.0;
        }
        state.flags |= LOADED;
        Ok(state)
    }

    /// Successful DuplicateMorphDef weight portion (RVA0x34ad0). The caller
    /// shares the immutable definition and duplicates the mutable mesh. Only
    /// raw weights are copied; native normalized storage starts unwritten.
    pub(crate) fn duplicate_morph_def(&self) -> Result<Self, SinglemorphError> {
        if self.flags & LOADED == 0 {
            return Err(SinglemorphError::Definition);
        }
        Ok(Self {
            flags: DIRTY | LOADED,
            levels: self.levels,
            regions: self.regions,
            weights: self.weights.clone(),
            normalized: None,
        })
    }
    /// Retained weight storage, excluding the containing state object.
    pub fn storage_bytes(&self) -> usize {
        (self.weights.capacity() + self.normalized.as_ref().map_or(0, Vec::capacity))
            * std::mem::size_of::<f32>()
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }
    pub fn level_count(&self) -> usize {
        self.levels
    }
    pub fn region_count(&self) -> usize {
        self.regions
    }
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }
    pub fn normalized_weights(&self) -> Option<&[f32]> {
        self.normalized.as_deref()
    }
    pub fn is_dirty(&self) -> bool {
        self.flags & DIRTY != 0
    }

    /// The validated SetWeightB portion (0x34460). The caller must also apply
    /// singlemorph::SetWeight's wrapping joints_changed increment and clear
    /// visual flag 0x40000. No clamping or finite-value rejection occurs here.
    pub fn set_weight(
        &mut self,
        region: usize,
        level: usize,
        weight: f32,
    ) -> Result<(), SinglemorphError> {
        if region >= self.regions || level >= self.levels {
            return Err(SinglemorphError::WeightIndex);
        }
        self.weights[region * self.levels + level] = weight;
        self.flags |= DIRTY;
        self.normalized = None;
        Ok(())
    }

    /// Original NormalizeWeights (0x344b0), bounded to finite normal f32 or
    /// signed zero for every operand/intermediate, under x87 PC24/RN.
    /// Sum absolute weights, preserve their signs, and select level zero when
    /// the sum is strictly below the original 1e-8 f32 threshold.
    pub fn normalize_weights(&mut self) -> Result<(), SinglemorphError> {
        let mut output = vec![0.0; self.weights.len()];
        if self.levels == 0 {
            self.normalized = Some(output);
            return Ok(());
        }
        for (input, output) in self
            .weights
            .chunks_exact(self.levels)
            .zip(output.chunks_exact_mut(self.levels))
        {
            let mut sum = 0.0;
            for weight in input {
                sum = add(sum, weight.abs())?;
            }
            if sum < THRESHOLD {
                output[0] = 1.0;
            } else {
                let reciprocal = 1.0 / sum;
                let widened = 1.0 / f64::from(sum);
                if !reciprocal.is_normal()
                    || widened < f64::from(f32::MIN_POSITIVE)
                    || widened > f64::from(f32::MAX)
                {
                    return Err(SinglemorphError::NumericDomain);
                }
                for (weight, destination) in input.iter().zip(output) {
                    *destination = mul(*weight, reciprocal)?;
                }
            }
        }
        self.normalized = Some(output);
        Ok(())
    }

    /// Original UpdateMorphLod (0x34570), using an available caller-owned vertex
    /// buffer. Regions overwrite in source order. Level zero always contributes;
    /// later levels contribute only for normalized weight strictly above 1e-8.
    /// Normals are summed without normalization; texture coordinates are kept.
    ///
    /// Returns true when the vertex update ran, clearing dirty. An unavailable
    /// LOD returns false after normalization and retains dirty, as in the native
    /// routine. Host errors preflight the whole result and publish no partial
    /// vertex stores; native behavior outside this admitted domain is unclaimed.
    pub fn update_morph_lod(
        &mut self,
        record: &MorphRecord,
        lod_index: usize,
        vertices: &mut [Vertex],
    ) -> Result<bool, SinglemorphError> {
        if self.flags & (DIRTY | LOADED) != (DIRTY | LOADED) {
            return Ok(false);
        }
        let definition = record
            .definition
            .as_ref()
            .ok_or(SinglemorphError::Definition)?;
        if usize::from(record.target_count) != self.levels
            || usize::from(definition.channel_count) != self.regions
        {
            return Err(SinglemorphError::Definition);
        }
        let mut normalized_state = self.clone();
        normalized_state.normalize_weights()?;
        let Some(lod) = definition.lods.get(lod_index) else {
            self.normalized = normalized_state.normalized;
            return Ok(false);
        };
        if lod.channels.len() != self.regions {
            return Err(SinglemorphError::Definition);
        }
        let normalized = normalized_state.normalized.as_ref().unwrap();
        let mut writes = Vec::new();
        for (region, channel) in lod.channels.iter().enumerate() {
            let count = usize::from(channel.vertex_count);
            if channel.samples.len() != count * self.levels {
                return Err(SinglemorphError::Definition);
            }
            if count != 0
                && (channel.index_mode.is_none_or(|mode| mode == 0)
                    || channel.vertex_indices.len() != count)
            {
                return Err(SinglemorphError::MissingVertexIndices);
            }
            let weights = &normalized[region * self.levels..(region + 1) * self.levels];
            for (samples, index) in channel
                .samples
                .chunks_exact(self.levels)
                .zip(&channel.vertex_indices)
            {
                let index = usize::from(*index);
                if index >= vertices.len() {
                    return Err(SinglemorphError::VertexIndex);
                }
                let mut position = [0.0; 3];
                let mut normal = [0.0; 3];
                for axis in 0..3 {
                    position[axis] = mul(weights[0], samples[0].position[axis])?;
                    normal[axis] = mul(samples[0].normal[axis], weights[0])?;
                }
                for (sample, weight) in samples[1..].iter().zip(&weights[1..]) {
                    if *weight > THRESHOLD {
                        for axis in 0..3 {
                            position[axis] =
                                add(position[axis], mul(sample.position[axis], *weight)?)?;
                            normal[axis] = add(normal[axis], mul(sample.normal[axis], *weight)?)?;
                        }
                    }
                }
                writes.push((index, position, normal));
            }
        }
        for (index, position, normal) in writes {
            vertices[index].position = position;
            vertices[index].normal = normal;
        }
        self.normalized = normalized_state.normalized;
        self.flags &= !DIRTY;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lhr_assets::{MorphChannelRecord, MorphDefinition, MorphLodRecord, MorphSampleRecord};

    fn record() -> MorphRecord {
        MorphRecord {
            target_count: 2,
            definition: Some(MorphDefinition {
                channel_count: 1,
                lods: vec![MorphLodRecord {
                    channels: vec![MorphChannelRecord {
                        vertex_count: 1,
                        samples: vec![
                            MorphSampleRecord {
                                position: [2.0, 4.0, 6.0],
                                normal: [1.0, 2.0, 3.0],
                            },
                            MorphSampleRecord {
                                position: [4.0, 8.0, 12.0],
                                normal: [3.0, 2.0, 1.0],
                            },
                        ],
                        index_mode: Some(2),
                        vertex_indices: vec![0],
                    }],
                }],
                trailer_words: [0; 10],
            }),
        }
    }
    fn vertices() -> Vec<Vertex> {
        vec![Vertex {
            position: [-1.; 3],
            normal: [-1.; 3],
            uv: [0.25, 0.75],
        }]
    }

    #[test]
    fn loaded_initial_weights_drive_morph_and_clean_update_preserves_cached_vertices() {
        let record = record();
        let mut state = SinglemorphState::from_record(&record).unwrap();
        assert_eq!(state.flags(), 3);
        assert_eq!(state.weights(), [1., 0.]);
        assert!(state.normalized_weights().is_none());
        let mut vertices = vertices();
        assert!(state.update_morph_lod(&record, 0, &mut vertices).unwrap());
        assert_eq!(vertices[0].position, [2., 4., 6.]);
        assert_eq!(vertices[0].uv, [0.25, 0.75]);
        vertices[0].position = [9.; 3];
        assert!(!state.update_morph_lod(&record, 0, &mut vertices).unwrap());
        assert_eq!(vertices[0].position, [9.; 3]);
    }

    #[test]
    fn signed_absolute_normalization_skips_negative_later_levels() {
        let record = record();
        let mut state = SinglemorphState::from_record(&record).unwrap();
        state.set_weight(0, 0, 1.).unwrap();
        state.set_weight(0, 1, -1.).unwrap();
        let mut vertices = vertices();
        state.update_morph_lod(&record, 0, &mut vertices).unwrap();
        assert_eq!(state.normalized_weights().unwrap(), [0.5, -0.5]);
        assert_eq!(vertices[0].position, [1., 2., 3.]);
        assert_eq!(vertices[0].normal, [0.5, 1., 1.5]);
    }

    #[test]
    fn threshold_is_strict_and_missing_lod_retains_dirty_after_normalization() {
        let record = record();
        let mut state = SinglemorphState::from_record(&record).unwrap();
        state
            .set_weight(0, 0, -f32::from_bits(THRESHOLD.to_bits() - 1))
            .unwrap();
        state.normalize_weights().unwrap();
        assert_eq!(state.normalized_weights().unwrap(), [1., 0.]);
        state.set_weight(0, 0, -THRESHOLD).unwrap();
        assert!(!state.update_morph_lod(&record, 1, &mut vertices()).unwrap());
        assert_eq!(state.normalized_weights().unwrap(), [-1., 0.]);
        assert!(state.is_dirty());
    }

    #[test]
    fn absent_native_indices_and_numeric_failure_publish_no_partial_vertices() {
        let mut record = record();
        let mut state = SinglemorphState::from_record(&record).unwrap();
        let mut vertices = vertices();
        record.definition.as_mut().unwrap().lods[0].channels[0].index_mode = Some(0);
        assert_eq!(
            state.update_morph_lod(&record, 0, &mut vertices),
            Err(SinglemorphError::MissingVertexIndices)
        );
        assert_eq!(vertices[0].position, [-1.; 3]);
        record.definition.as_mut().unwrap().lods[0].channels[0].index_mode = Some(1);
        state.set_weight(0, 1, f32::NAN).unwrap();
        assert_eq!(
            state.update_morph_lod(&record, 0, &mut vertices),
            Err(SinglemorphError::NumericDomain)
        );
        assert_eq!(vertices[0].position, [-1.; 3]);
        assert!(state.is_dirty());
    }

    #[test]
    fn zero_target_projection_is_dirty_unloaded_and_rejects_weights() {
        let record = MorphRecord {
            target_count: 0,
            definition: None,
        };
        let mut state = SinglemorphState::from_record(&record).unwrap();
        assert_eq!(state.flags(), 1);
        assert_eq!(
            state.set_weight(0, 0, 1.),
            Err(SinglemorphError::WeightIndex)
        );
        assert!(!state.update_morph_lod(&record, 0, &mut vertices()).unwrap());
    }
}
