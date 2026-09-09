//! Ordonnancement déterministe d'un pipeline Transformer par couches.
//!
//! Le transport LAN reste séparé : ce module valide l'ordre et les frontières,
//! puis délègue chaque activation à un exécuteur local ou distant.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerStage {
    pub node_id: String,
    pub first_layer: u32,
    pub last_layer: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerPipelinePlan {
    pub total_layers: u32,
    pub stages: Vec<LayerStage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerPipelineMetrics {
    pub layers_executed: u32,
    pub transfers: u32,
    pub cancelled: bool,
}

/// Partition a Transformer into contiguous, non-empty ranges.
///
/// The first ranges receive one extra layer when the division is uneven. The
/// function is deliberately transport-neutral so UI and daemon callers use
/// the same deterministic partitioning rule.
pub fn partition_layer_stages(
    total_layers: u32,
    node_ids: &[String],
) -> Result<Vec<LayerStage>, String> {
    if total_layers == 0 || node_ids.is_empty() {
        return Err("partition de couches vide".into());
    }
    let worker_count = node_ids.len().min(total_layers as usize) as u32;
    let base_layers = total_layers / worker_count;
    let remainder = total_layers % worker_count;
    let mut cursor = 0u32;
    let mut stages = Vec::with_capacity(worker_count as usize);
    for (index, node_id) in node_ids.iter().take(worker_count as usize).enumerate() {
        if node_id.trim().is_empty() {
            return Err("identifiant de worker vide".into());
        }
        let layer_count = base_layers + u32::from((index as u32) < remainder);
        let first_layer = cursor;
        let last_layer = cursor + layer_count - 1;
        cursor += layer_count;
        stages.push(LayerStage {
            node_id: node_id.clone(),
            first_layer,
            last_layer,
        });
    }
    LayerPipelinePlan::new(total_layers, stages.clone())?;
    Ok(stages)
}

pub trait LayerPipelineExecutor {
    fn execute_layer(
        &mut self,
        node_id: &str,
        layer_index: u32,
        sequence: u32,
        activation: Vec<u8>,
    ) -> Result<Vec<u8>, String>;
}

impl LayerPipelinePlan {
    pub fn new(total_layers: u32, stages: Vec<LayerStage>) -> Result<Self, String> {
        if total_layers == 0 || stages.is_empty() {
            return Err("pipeline de couches vide".into());
        }
        let mut expected = 0u32;
        for stage in &stages {
            if stage.node_id.trim().is_empty()
                || stage.first_layer != expected
                || stage.first_layer > stage.last_layer
                || stage.last_layer >= total_layers
            {
                return Err("segments de pipeline non contigus ou invalides".into());
            }
            expected = stage.last_layer.saturating_add(1);
        }
        if expected != total_layers {
            return Err("pipeline incomplet".into());
        }
        Ok(Self {
            total_layers,
            stages,
        })
    }

    pub fn run<E: LayerPipelineExecutor>(
        &self,
        executor: &mut E,
        mut activation: Vec<u8>,
        sequence: u32,
        cancelled: impl Fn() -> bool,
    ) -> Result<(Vec<u8>, LayerPipelineMetrics), String> {
        if activation.is_empty() {
            return Err("activation initiale vide".into());
        }
        let mut layers_executed = 0;
        let mut transfers = 0;
        for stage in &self.stages {
            for layer_index in stage.first_layer..=stage.last_layer {
                if cancelled() {
                    return Ok((
                        activation,
                        LayerPipelineMetrics {
                            layers_executed,
                            transfers,
                            cancelled: true,
                        },
                    ));
                }
                activation = executor.execute_layer(
                    &stage.node_id,
                    layer_index,
                    sequence.saturating_add(layers_executed),
                    activation,
                )?;
                if activation.is_empty() {
                    return Err("activation vide après une couche".into());
                }
                layers_executed = layers_executed.saturating_add(1);
            }
            if stage.last_layer < self.total_layers - 1 {
                transfers = transfers.saturating_add(1);
            }
        }
        Ok((
            activation,
            LayerPipelineMetrics {
                layers_executed,
                transfers,
                cancelled: false,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Trace(Vec<(String, u32, u32)>);

    impl LayerPipelineExecutor for Trace {
        fn execute_layer(
            &mut self,
            node_id: &str,
            layer_index: u32,
            sequence: u32,
            mut activation: Vec<u8>,
        ) -> Result<Vec<u8>, String> {
            self.0.push((node_id.to_string(), layer_index, sequence));
            activation.push(layer_index as u8);
            Ok(activation)
        }
    }

    #[test]
    fn pipeline_exécute_les_segments_dans_l_ordre() {
        let plan = LayerPipelinePlan::new(
            4,
            vec![
                LayerStage {
                    node_id: "node-a".into(),
                    first_layer: 0,
                    last_layer: 1,
                },
                LayerStage {
                    node_id: "node-b".into(),
                    first_layer: 2,
                    last_layer: 3,
                },
            ],
        )
        .unwrap();
        let mut trace = Trace(Vec::new());
        let (activation, metrics) = plan.run(&mut trace, vec![1], 10, || false).unwrap();
        assert_eq!(
            trace.0,
            vec![
                ("node-a".into(), 0, 10),
                ("node-a".into(), 1, 11),
                ("node-b".into(), 2, 12),
                ("node-b".into(), 3, 13)
            ]
        );
        assert_eq!(activation, vec![1, 0, 1, 2, 3]);
        assert_eq!(
            metrics,
            LayerPipelineMetrics {
                layers_executed: 4,
                transfers: 1,
                cancelled: false
            }
        );
    }

    #[test]
    fn pipeline_refuse_un_trou_et_s_arrete_sur_annulation() {
        assert!(LayerPipelinePlan::new(
            3,
            vec![LayerStage {
                node_id: "node".into(),
                first_layer: 0,
                last_layer: 1
            }],
        )
        .is_err());
        let plan = LayerPipelinePlan::new(
            2,
            vec![LayerStage {
                node_id: "node".into(),
                first_layer: 0,
                last_layer: 1,
            }],
        )
        .unwrap();
        let mut trace = Trace(Vec::new());
        let (_, metrics) = plan.run(&mut trace, vec![1], 0, || true).unwrap();
        assert_eq!(metrics.layers_executed, 0);
        assert!(metrics.cancelled);
    }

    #[test]
    fn partition_evenly_preserves_order_and_covers_every_layer() {
        let nodes = vec!["a".into(), "b".into(), "c".into()];
        let stages = partition_layer_stages(8, &nodes).unwrap();
        assert_eq!(
            stages,
            vec![
                LayerStage {
                    node_id: "a".into(),
                    first_layer: 0,
                    last_layer: 2
                },
                LayerStage {
                    node_id: "b".into(),
                    first_layer: 3,
                    last_layer: 5
                },
                LayerStage {
                    node_id: "c".into(),
                    first_layer: 6,
                    last_layer: 7
                },
            ]
        );
        assert!(partition_layer_stages(2, &nodes)
            .unwrap()
            .iter()
            .all(|stage| { stage.first_layer == stage.last_layer }));
    }
}
