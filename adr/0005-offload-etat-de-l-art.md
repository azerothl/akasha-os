# ADR 0005 — Offload CPU/GPU/RAM/disque : état de l'art et décisions pour P1

> Date : 12/08/2026
> Statut : accepté
> Références : `adr/0002-model-placement.md`, `specs-techniques.md` §3.5, §3.5.8, §15 ; Gate P1 (risque « perf offload disque »)
> Sources primaires : docs officielles Accelerate / DeepSpeed, README AirLLM et onnxruntime-genai, papiers FlexGen (arXiv 2303.06865) et PowerInfer (arXiv 2312.12456, SOSP 2024)

---

## 1. Contexte

Avant P1 (Placement Manager réel sur llama.cpp), nous avons analysé comment les
frameworks existants font l'offload CPU/GPU/RAM/disque, autour de 4 questions :

1. Mécanismes de recouvrement calcul/transfert (pinned memory, copy streams, aio) ;
2. Crossover « couches RAM calculées sur CPU » vs « uploadées vers GPU » selon batch ;
3. Politiques de placement du KV cache sous long contexte ;
4. Anti-patterns à ne pas reproduire.

Verdict de l'étude : **nos hypothèses P0 sont confirmées sur les points
structurels**, avec un ajustement de constante (`eff_disk`) et des exigences
d'implémentation précises pour le Weight Store de P1.2.

## 2. Analyse par framework

### 2.1 Hugging Face Accelerate — l'anti-pattern de référence

Mécanisme (docs officielles « big model inference ») :

- `device_map="auto"` : remplissage **glouton séquentiel** — GPU d'abord, puis
  RAM CPU, puis disque en **tensors memory-mappés** ; granularité = sous-module
  (`no_split_module_classes`) ; aucun scoring de hotness.
- Exécution : hooks synchrones — les poids CPU sont « mis sur GPU juste avant
  le forward et nettoyés juste après » ; les poids disque sont « chargés en RAM
  puis mis sur GPU ». La doc dit explicitement : *« there is no pre-fetching
  (yet) »* et *« hard-drive offloading might be very slow »* sans NVMe rapide.
- Multi-GPU : « naïf, un seul GPU travaille à la fois ».

Enseignements pour nous :

- C'est le cas **additif** de notre modèle de coût (`t_compute + t_stream` au
  lieu de `max(...)`) : il quantifie ce que coûte l'absence de prefetch et
  valide a posteriori le choix §3.5.4 comme **exigence** et non option.
- Le remplissage glouton sans hotness confirme que l'ordre des couches est
  sans effet sur le coût — notre note ADR 0002 tient.
- Le mmap disque valide la faisabilité du Weight Store §3.5.8.

### 2.2 DeepSpeed — la preuve du recouvrement

- **ZeRO-Inference** stream les poids depuis CPU/NVMe via un *parameter
  swapper* adossé à **DeepNVMe** : I/O asynchrones (`libaio`, `async_pread` +
  `wait()`), **tensors épinglés obligatoires pour le DMA**, parallélisme
  intra-op, `queue_depth`/`block_size` réglables (valeurs tunées publiées :
  block 1 Mio, queue 32, intra-op 8).
- Mesure publiée (workstation 2× A6000, NVMe CS3040 pic 5,6 GB/s) : **3,69
  GB/s lus en lecture réelle ≈ 66 % du pic**. C'est le meilleur point
  d'étalonnage public pour `eff_disk`.
- ZeRO-Offload / ZeRO-Infinity : training (optimizer/gradients/params vers
  CPU/NVMe) — hors scope, mais même plomberie.
- DeepSpeed-FastGen : continuous batching — référence directe pour P5.

Enseignement clé : le streaming de poids vers le GPU avec recouvrement est un
mécanisme de **régime throughput** (gros batchs). En latence single-stream,
calculer les couches RAM sur CPU (façon llama.cpp `-ngl`) reste le bon défaut.

### 2.3 ONNX Runtime GenAI — hors sujet pour l'offload, utile ailleurs

Je n'ai trouvé **aucun composant nommé « LMR »** dans l'écosystème ONNX
Runtime (à confirmer par le demandeur). ORT GenAI fournit la boucle générative
complète (sampling, KV cache, grammar, multi-LoRA, continuous decoding en
développement) et une matrice matérielle large (CPU/CUDA/DirectML/TRT-RTX/
OpenVINO/QNN/WebGPU) — il propulse Foundry Local / Windows ML. **Aucun
mécanisme d'offload disque.** Pertinence pour nous : backend `AccelDevice`
candidat pour NPU/DirectML côté Windows — **P5, pas P1**.

### 2.4 llama.cpp — la calibration directe de P1

- `mmap` par défaut : l'offload disque « gratuit » via page cache OS ;
  `--no-mmap` pour comparer ; `-ngl N` = split VRAM contigu + couches RAM
  calculées sur CPU. C'est le comportement que notre modèle de coût suppose.
- **En P1, llama.cpp via FFI est à la fois notre backend et notre banc de
  mesure** : mmap vs no-mmap nous donnera la première preuve réelle du
  streaming disque (hypothèse `max(compute, stream)`).

### 2.5 AirLLM — notre scénario S3 en production

- Streaming couche-par-couche (1 couche en VRAM à la fois) : 70B sur 4 Go,
  405B sur 8 Go ; décomposition du modèle en shards de couches sur disque.
- **Prefetch activé par défaut depuis v2.5 : +10 % mesurés.** Le gain modeste
  confirme que le régime disque reste stream-bound (`max` ≈ `stream`) — le
  prefetch ne supprime pas le goulot, il évite de l'aggraver.
- **Compression block-wise 4/8 bits : ×3** sur le régime disque — preuve que
  la quantization est un levier de *placement* (réduit directement les octets
  streamés), pas seulement de qualité/taille. Soutient le « downgrade Q6→Q4
  sous pression » de §3.5.8.
- MoE : streaming **par expert** (seuls les experts routés) — granularité plus
  fine que la couche, à retenir pour les modèles MoE/FP8.

### 2.6 FlexGen — la formulation générale (throughput)

- Agrège GPU+CPU+disque pour poids **et KV cache** (compressés 4 bits),
  placement trouvé par **programmation linéaire**, schedule zig-zag par blocs.
  OPT-175B sur un seul GPU 16 Go : 1 tok/s en batch effectif 144.
- Cible le throughput latency-insensitive : la formulation LP est
  surdimensionnée pour P1, mais c'est **l'option documentée pour P5**
  (multi-modèles, multi-GPU, continuous batching).

### 2.7 PowerInfer — granularité neuronale (SOSP 2024)

- Split hot/cold au niveau **neurones** (loi de puissance d'activation) :
  hot préchargés GPU, cold calculés CPU, avec prédicteurs d'activation.
  Jusqu'à ×11,7 vs llama.cpp sur RTX 4090 ; OPT-30B à 82 % du débit d'une A100.
- Axe orthogonal à notre granularité « couche » : si nos hypothèses de hotness
  par couche se révèlent limites sur gros modèles consumer, c'est la piste
  d'upgrade (P5+). Hors scope P1.

## 3. Réponses aux 4 questions

1. **Recouvrement** : confirmé indispensable et documenté — pinned buffers +
   stream de copie async + I/O profonds (queue ≥ 32, blocs ≥ 1 Mio).
   L'hypothèse `max(compute, stream)` tient **à condition que le prefetch soit
   réellement implémenté** (sinon on retombe sur le cas Accelerate, additif).
2. **Crossover CPU-compute vs upload-GPU** : CPU-compute en latence
   single-stream (défaut P1) ; upload-GPU streamé ne paie qu'en batchs
   (ZeRO/FlexGen). Un *execution-mode switch* dépendant du batch sera une
   affaire de **P5** (continuous batching).
3. **KV cache** : FlexGen montre que l'offload KV CPU/disque (compressé) est
   viable en throughput ; en interactif, KV reste sur les tiers rapides
   (notre politique §3.5.3 confirmée). Noter la granularité **expert** pour MoE.
4. **Anti-patterns** : hooks synchrones par couche (Accelerate), copies
   pageables sans pinned memory, device maps naïfs multi-GPU, et croire que le
   prefetch seul rend le disque rapide (AirLLM : +10 % seulement).

## 4. Décisions

- **D1** — P1.2 (Weight Store) : mmap + buffers épinglés + stream de copie
  dédié + lecture profonde asynchrone ; paramètres de départ inspirés de
  DeepNVMe (blocs 1 Mio, queue 32, intra-op 8). Mesure systématique mmap vs
  lecture explicite dès le premier prototype llama.cpp FFI.
- **D2** — `eff_disk` : **0,80 → 0,65** (DeepNVMe mesuré 66 % du pic NVMe sur
  workload réel tuné). Modèle de coût et ADR 0002 ajustés. `host_probe`
  gagnera une mesure disque en P1 pour calibration par hôte.
- **D3** — Modèle de coût : pas de changement structurel. Le mode
  « upload-GPU streamé » est documenté comme extension P5, avec switch par
  taille de batch.
- **D4** — Recommandation specs : promouvoir le downgrade de quantization
  sous pression (§3.5.8) de Could à **Should** au prochain bump de version
  (preuve AirLLM ×3 / FlexGen 4-bit). À acter dans `specs-techniques.md`.
- **D5** — Granularité expert (MoE) et neuronale (PowerInfer) consignées
  comme pistes P5+, non bloquantes pour P1.
- **D6** — ORT GenAI rangé comme candidat backend `AccelDevice` NPU/DirectML
  pour P5 ; aucune action P1.

## 5. Conséquences

- `crates/aos-placement/src/cost.rs` : `eff_disk` passe à 0,65 (commentaires
  mis à jour) ; le scénario S3 (ratio ≥ 25 % du full-RAM) continue de passer.
- `adr/0002-model-placement.md` : table d'étalonnage amendée.
- La liste des ADRs dans `plan-developpement-phases.md` et
  `specs-techniques.md` annexe C est complétée de la présente référence.

## 6. Sources

- Hugging Face Accelerate — *Concept guides / Big model inference* (docs officielles).
- DeepSpeed — tutoriel *DeepNVMe* (paramètres tunés + mesure 3,69 GB/s) et
  tutoriel *Inference* (ZeRO-Inference / parameter swapper).
- AirLLM — README (`lyogavin/airllm`, prefetch v2.5 +10 %, compression ×3, MoE par expert).
- FlexGen — arXiv:2303.06865 (LP placement, KV 4-bit, OPT-175B sur GPU 16 Go).
- PowerInfer — arXiv:2312.12456, SOSP 2024 (hot/cold neurons, ×11,7 vs llama.cpp sur 4090).
- onnxruntime-genai — README Microsoft (matrice matérielle, pas d'offload disque).
