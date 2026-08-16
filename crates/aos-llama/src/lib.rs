//! # aos-llama — Backend Manager llama.cpp via FFI (P1.1)
//!
//! Wrapper sûr et minimal sur `llama-cpp-sys-2` (API C de llama.cpp),
//! implémentant l'API unifiée interne des backends (specs-techniques §3.3) :
//! chargement avec **contrôle de placement** (`n_gpu_layers`, mode mmap),
//! inférence en streaming via callback, annulation coopérative
//! (`abort_callback`), métriques de timing.
//!
//! ## Correspondance plan de placement → paramètres llama.cpp (P1.2)
//!
//! | Plan (aos-placement) | llama.cpp |
//! |----------------------|-----------|
//! | couches en VRAM | `n_gpu_layers` (préfixe contigu, coût identique, ADR 0002) |
//! | KV en VRAM | `offload_kqv = true` |
//! | tier DISK | `load_mode = MMAP` (page-in paresseux) ; `DIRECT_IO` en expérimental |
//! | couches RAM | calculées CPU (`n_threads`) — comportement natif llama.cpp |
//!
//! Non thread-safe par construction : un [`LlamaContext`] = un decode à la
//! fois. Le scheduler (`aos-modeld`) y envoie un **batch** de jusqu'à
//! `n_seq_max` séquences (continuous batching P5.1).

#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_void, CStr, CString};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;

use llama_cpp_sys_2 as sys;

#[derive(Debug, Error, Clone)]
pub enum LlamaError {
    #[error("chemin modèle invalide (non UTF-8 / NUL)")]
    InvalidPath,
    #[error("échec de chargement du modèle: {0}")]
    ModelLoad(String),
    #[error("échec de création du contexte")]
    ContextCreate,
    #[error("échec d'application du chat template")]
    ChatTemplate,
    #[error("échec de tokenisation")]
    Tokenize,
    #[error("échec llama_decode (code {0})")]
    Decode(i32),
    #[error(
        "le prompt ne tient pas dans le contexte (prompt={prompt} + réserve_gen={gen_reserve} = {need} tokens > ctx={ctx})"
    )]
    PromptTooLong {
        prompt: usize,
        ctx: u32,
        gen_reserve: u32,
        need: usize,
    },
}

impl LlamaError {
    pub fn prompt_too_long(prompt: usize, ctx: u32, max_tokens: u32) -> Self {
        let gen_reserve = max_tokens.saturating_add(8);
        let need = prompt.saturating_add(gen_reserve as usize);
        Self::PromptTooLong {
            prompt,
            ctx,
            gen_reserve,
            need,
        }
    }
}

/// Mode de chargement des poids (mapping tier DISK, §3.5.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    /// mmap + page cache OS (défaut ; lazy page-in depuis disque).
    Mmap,
    /// mmap + mlock (épinglé en RAM).
    MmapMlock,
    /// Chargement intégral en RAM (pas de mmap).
    NoMmap,
    /// Direct I/O (expérimental llama.cpp).
    DirectIo,
}

/// Options de chargement = sortie concrète du Placement Manager réel.
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Couches en VRAM (plan : `n_layers_on(Tier::Vram)`).
    pub n_gpu_layers: i32,
    pub load_mode: LoadMode,
    /// KV cache en VRAM (plan : KV sur tier VRAM).
    pub offload_kqv: bool,
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub n_threads: i32,
    pub flash_attn: bool,
    /// Contexte d'embeddings (pooling mean) au lieu de génération.
    pub embeddings: bool,
    /// Séquences simultanées (continuous batching P5.1). 1 = une à la fois.
    pub n_seq_max: u32,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            n_gpu_layers: 0,
            load_mode: LoadMode::Mmap,
            offload_kqv: true,
            n_ctx: 4096,
            n_batch: 512,
            n_ubatch: 512,
            n_threads: 8,
            flash_attn: true,
            embeddings: false,
            n_seq_max: 1,
        }
    }
}

/// Backend global (init une fois par processus).
pub struct LlamaBackend {
    _private: (),
}

impl LlamaBackend {
    pub fn init() -> Self {
        unsafe {
            sys::llama_backend_init();
        }
        Self { _private: () }
    }

    /// GPU avec offload supporté par ce build ?
    pub fn supports_gpu_offload() -> bool {
        unsafe { sys::llama_supports_gpu_offload() }
    }

    pub fn max_devices() -> usize {
        unsafe { sys::llama_max_devices() }
    }
}

impl Drop for LlamaBackend {
    fn drop(&mut self) {
        unsafe { sys::llama_backend_free() };
    }
}

/// Modèle chargé (poids mappés/alloués selon [`LoadOptions`]).
pub struct LlamaModel {
    ptr: *mut sys::llama_model,
    pub n_layer: i32,
    pub size_bytes: u64,
    chat_template: Option<CString>,
}

// Les pointeurs llama.cpp ne sont pas partagés entre threads sans
// synchronisation — le scheduler de modeld sérialise l'accès au contexte,
// donc le transfert entre threads est sûr ici (usage exclusif).
unsafe impl Send for LlamaModel {}
unsafe impl Sync for LlamaModel {}

impl LlamaModel {
    pub fn load(path: &Path, opts: &LoadOptions) -> Result<Self, LlamaError> {
        let cpath = CString::new(path.to_str().ok_or(LlamaError::InvalidPath)?)
            .map_err(|_| LlamaError::InvalidPath)?;
        let mut params = unsafe { sys::llama_model_default_params() };
        params.n_gpu_layers = opts.n_gpu_layers;
        params.split_mode = sys::LLAMA_SPLIT_MODE_LAYER;
        params.load_mode = match opts.load_mode {
            LoadMode::Mmap => sys::LLAMA_LOAD_MODE_MMAP,
            LoadMode::MmapMlock => sys::LLAMA_LOAD_MODE_MMAP_MLOCK,
            LoadMode::NoMmap => sys::LLAMA_LOAD_MODE_NONE,
            LoadMode::DirectIo => sys::LLAMA_LOAD_MODE_DIRECT_IO,
        };
        let ptr = unsafe { sys::llama_model_load_from_file(cpath.as_ptr(), params) };
        if ptr.is_null() {
            return Err(LlamaError::ModelLoad(path.display().to_string()));
        }
        let n_layer = unsafe { sys::llama_model_n_layer(ptr) };
        let size_bytes = unsafe { sys::llama_model_size(ptr) };
        let chat_template = unsafe {
            let t = sys::llama_model_chat_template(ptr, std::ptr::null());
            if t.is_null() {
                None
            } else {
                Some(CStr::from_ptr(t).to_owned())
            }
        };
        Ok(Self {
            ptr,
            n_layer,
            size_bytes,
            chat_template,
        })
    }
}

impl Drop for LlamaModel {
    fn drop(&mut self) {
        unsafe { sys::llama_model_free(self.ptr) };
    }
}

/// Paramètres de génération.
#[derive(Debug, Clone)]
pub struct GenParams {
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub seed: u32,
}

/// Raison d'arrêt d'une génération.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Eog,
    MaxTokens,
    Aborted,
}

/// Statistiques d'une génération (Metrics Exporter, F-PLC-08).
#[derive(Debug, Clone)]
pub struct GenStats {
    pub prompt_tokens: u32,
    pub generated_tokens: u32,
    pub ttft_ms: f64,
    pub tok_s: f64,
    pub stopped: StopReason,
}

/// Callback d'annulation ggml (appelé par le scheduler llama.cpp).
unsafe extern "C" fn abort_trampoline(data: *mut c_void) -> bool {
    let flag = unsafe { &*(data as *const AtomicBool) };
    flag.load(Ordering::SeqCst)
}

/// Job d'un batch continu (P5.1).
pub struct BatchItem {
    pub messages: Vec<(String, String)>,
    pub params: GenParams,
    pub abort: Arc<AtomicBool>,
}

/// Contexte d'inférence. `n_seq_max` > 1 active le continuous batching.
pub struct LlamaContext {
    ptr: *mut sys::llama_context,
    model: Arc<LlamaModel>,
    abort: Arc<AtomicBool>,
    n_ctx: u32,
    n_batch: u32,
    n_seq_max: u32,
}

unsafe impl Send for LlamaContext {}

impl LlamaContext {
    pub fn new(model: Arc<LlamaModel>, opts: &LoadOptions) -> Result<Self, LlamaError> {
        let mut params = unsafe { sys::llama_context_default_params() };
        params.n_ctx = opts.n_ctx;
        params.n_batch = opts.n_batch;
        params.n_ubatch = opts.n_ubatch;
        params.n_seq_max = opts.n_seq_max.max(1);
        params.n_threads = opts.n_threads;
        params.n_threads_batch = opts.n_threads;
        params.offload_kqv = opts.offload_kqv;
        // Cache KV unifié : un seul stream d'attention (meilleur decode multi-seq).
        params.kv_unified = true;
        params.embeddings = opts.embeddings;
        if opts.embeddings {
            params.pooling_type = sys::LLAMA_POOLING_TYPE_MEAN;
        }
        params.flash_attn_type = if opts.flash_attn {
            sys::LLAMA_FLASH_ATTN_TYPE_ENABLED
        } else {
            sys::LLAMA_FLASH_ATTN_TYPE_DISABLED
        };
        let abort = Arc::new(AtomicBool::new(false));
        // Pointeur stable vers le flag pour la callback C : l'AtomicBool vit
        // dans l'Arc stocké dans le contexte, libéré après llama_free.
        params.abort_callback = Some(abort_trampoline);
        params.abort_callback_data = Arc::as_ptr(&abort) as *mut c_void;

        let ptr = unsafe { sys::llama_init_from_model(model.ptr, params) };
        if ptr.is_null() {
            return Err(LlamaError::ContextCreate);
        }
        Ok(Self {
            ptr,
            model,
            abort,
            n_ctx: opts.n_ctx,
            n_batch: opts.n_batch.max(1),
            n_seq_max: opts.n_seq_max.max(1),
        })
    }

    /// Demande d'annulation (prend effet à la frontière de token courante).
    pub fn abort(&self) {
        self.abort.store(true, Ordering::SeqCst);
    }

    pub fn abort_handle(&self) -> Arc<AtomicBool> {
        self.abort.clone()
    }

    /// Applique le chat template du modèle (fallback ChatML si absent).
    fn render_prompt(&self, messages: &[(String, String)]) -> Result<String, LlamaError> {
        let model = &self.model;
        match &model.chat_template {
            Some(tmpl) => {
                let croles: Vec<CString> = messages
                    .iter()
                    .map(|(r, _)| CString::new(r.as_str()).unwrap_or_default())
                    .collect();
                let ccontents: Vec<CString> = messages
                    .iter()
                    .map(|(_, c)| CString::new(c.as_str()).unwrap_or_default())
                    .collect();
                let raw: Vec<sys::llama_chat_message> = croles
                    .iter()
                    .zip(&ccontents)
                    .map(|(r, c)| sys::llama_chat_message {
                        role: r.as_ptr(),
                        content: c.as_ptr(),
                    })
                    .collect();
                let mut cap: usize = messages
                    .iter()
                    .map(|(r, c)| r.len() + c.len())
                    .sum::<usize>()
                    + 8 * 1024;
                loop {
                    let mut buf = vec![0i8; cap];
                    let n = unsafe {
                        sys::llama_chat_apply_template(
                            tmpl.as_ptr(),
                            raw.as_ptr(),
                            raw.len(),
                            true,
                            buf.as_mut_ptr(),
                            buf.len() as i32,
                        )
                    };
                    if n < 0 {
                        return Err(LlamaError::ChatTemplate);
                    }
                    if n as usize <= buf.len() {
                        let bytes: Vec<u8> = buf[..n as usize].iter().map(|b| *b as u8).collect();
                        let rendered = String::from_utf8_lossy(&bytes).into_owned();
                        let tmpl_s = tmpl.to_string_lossy();
                        return Ok(suppress_hybrid_thinking(&rendered, &tmpl_s));
                    }
                    cap = n as usize + 1024;
                }
            }
            None => {
                // Fallback ChatML minimal.
                let mut s = String::new();
                for (r, c) in messages {
                    s.push_str(&format!("<|im_start|>{r}\n{c}<|im_end|>\n"));
                }
                s.push_str("<|im_start|>assistant\n");
                Ok(s)
            }
        }
    }

    fn tokenize(&self, text: &str, add_special: bool) -> Result<Vec<sys::llama_token>, LlamaError> {
        let vocab = unsafe { sys::llama_model_get_vocab(self.model.ptr) };
        let ctext = CString::new(text).map_err(|_| LlamaError::Tokenize)?;
        let mut cap = text.len() / 2 + 64;
        loop {
            let mut tokens = vec![0 as sys::llama_token; cap];
            let n = unsafe {
                sys::llama_tokenize(
                    vocab,
                    ctext.as_ptr(),
                    ctext.as_bytes().len() as i32,
                    tokens.as_mut_ptr(),
                    tokens.len() as i32,
                    add_special,
                    true,
                )
            };
            if n >= 0 {
                tokens.truncate(n as usize);
                return Ok(tokens);
            }
            // n négatif = capacité requise.
            cap = (-n) as usize + 64;
        }
    }

    fn token_to_piece(&self, token: sys::llama_token) -> String {
        let vocab = unsafe { sys::llama_model_get_vocab(self.model.ptr) };
        let mut buf = [0i8; 256];
        let n = unsafe {
            sys::llama_detokenize(
                vocab,
                &token,
                1,
                buf.as_mut_ptr(),
                buf.len() as i32,
                true,
                false,
            )
        };
        if n <= 0 {
            return String::new();
        }
        let bytes: Vec<u8> = buf[..n as usize].iter().map(|b| *b as u8).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Génère une réponse en streaming.
    ///
    /// `on_delta` est appelé pour chaque fragment de texte ; retourner `false`
    /// demande l'arrêt coopératif (annulation à la frontière de token, §3.6).
    pub fn generate(
        &mut self,
        messages: &[(String, String)],
        params: &GenParams,
        mut on_delta: impl FnMut(&str) -> bool,
    ) -> Result<GenStats, LlamaError> {
        self.abort.store(false, Ordering::SeqCst);

        let prompt = self.render_prompt(messages)?;
        let prompt_tokens = self.tokenize(&prompt, false)?;
        let n_prompt = prompt_tokens.len();
        if n_prompt + params.max_tokens as usize + 8 > self.n_ctx_seq() as usize {
            return Err(LlamaError::prompt_too_long(
                n_prompt,
                self.n_ctx_seq(),
                params.max_tokens,
            ));
        }

        // KV cache vierge pour cette requête (v1 single-sequence).
        unsafe {
            let mem = sys::llama_get_memory(self.ptr);
            sys::llama_memory_clear(mem, true);
        }

        // Sampler chain : top_p → temp → dist.
        let sparams = unsafe { sys::llama_sampler_chain_default_params() };
        let smpl = unsafe { sys::llama_sampler_chain_init(sparams) };
        unsafe {
            sys::llama_sampler_chain_add(smpl, sys::llama_sampler_init_top_p(params.top_p, 1));
            sys::llama_sampler_chain_add(smpl, sys::llama_sampler_init_temp(params.temperature));
            sys::llama_sampler_chain_add(smpl, sys::llama_sampler_init_dist(params.seed));
        }
        let vocab = unsafe { sys::llama_model_get_vocab(self.model.ptr) };

        let t_start = Instant::now();

        // Prefill découpé par n_batch — `llama_batch_get_one(n_prompt)` assert
        // si n_prompt > n_batch (historique chat long → crash modeld).
        let mut batch = unsafe { sys::llama_batch_init(self.n_batch as i32, 0, 1) };
        let mut off = 0usize;
        while off < n_prompt {
            batch.n_tokens = 0;
            let take = (n_prompt - off).min(self.n_batch as usize);
            for j in 0..take {
                let is_last = off + j + 1 == n_prompt;
                unsafe {
                    Self::batch_add(
                        &mut batch,
                        prompt_tokens[off + j],
                        (off + j) as sys::llama_pos,
                        0,
                        is_last,
                    );
                }
            }
            let rc = unsafe { sys::llama_decode(self.ptr, batch) };
            if rc != 0 {
                unsafe {
                    sys::llama_batch_free(batch);
                    sys::llama_sampler_free(smpl);
                }
                return Err(LlamaError::Decode(rc));
            }
            off += take;
        }
        unsafe { sys::llama_batch_free(batch) };

        let mut generated = 0u32;
        let mut ttft_ms = f64::MAX;
        let mut cur: sys::llama_token;

        // Premier token.
        cur = unsafe { sys::llama_sampler_sample(smpl, self.ptr, -1) };
        let stopped = loop {
            if self.abort.load(Ordering::SeqCst) {
                break StopReason::Aborted;
            }
            if unsafe { sys::llama_vocab_is_eog(vocab, cur) } {
                break StopReason::Eog;
            }
            if generated >= params.max_tokens {
                break StopReason::MaxTokens;
            }
            if ttft_ms == f64::MAX {
                ttft_ms = t_start.elapsed().as_secs_f64() * 1000.0;
            }
            let piece = self.token_to_piece(cur);
            if !piece.is_empty() && !on_delta(&piece) {
                break StopReason::Aborted;
            }
            generated += 1;

            // Decode du prochain token.
            let mut tok = cur;
            let batch = unsafe { sys::llama_batch_get_one(&mut tok, 1) };
            let rc = unsafe { sys::llama_decode(self.ptr, batch) };
            if rc != 0 {
                unsafe { sys::llama_sampler_free(smpl) };
                return Err(LlamaError::Decode(rc));
            }
            cur = unsafe { sys::llama_sampler_sample(smpl, self.ptr, -1) };
        };

        unsafe { sys::llama_sampler_free(smpl) };

        let total = t_start.elapsed().as_secs_f64();
        let ttft = if ttft_ms == f64::MAX {
            total * 1000.0
        } else {
            ttft_ms
        };
        let decode_s = (total - ttft / 1000.0).max(1e-6);
        Ok(GenStats {
            prompt_tokens: n_prompt as u32,
            generated_tokens: generated,
            ttft_ms: ttft,
            tok_s: generated as f64 / decode_s,
            stopped,
        })
    }

    pub fn n_seq_max(&self) -> u32 {
        self.n_seq_max
    }

    /// Budget de tokens par séquence (n_ctx / n_seq_max).
    fn n_ctx_seq(&self) -> u32 {
        (self.n_ctx / self.n_seq_max.max(1)).max(1)
    }

    unsafe fn batch_add(
        batch: &mut sys::llama_batch,
        token: sys::llama_token,
        pos: sys::llama_pos,
        seq_id: sys::llama_seq_id,
        logits: bool,
    ) {
        let i = batch.n_tokens as usize;
        *batch.token.add(i) = token;
        *batch.pos.add(i) = pos;
        *batch.n_seq_id.add(i) = 1;
        *(*batch.seq_id.add(i)) = seq_id;
        *batch.logits.add(i) = i8::from(logits);
        batch.n_tokens += 1;
    }

    fn make_sampler(params: &GenParams) -> *mut sys::llama_sampler {
        let sparams = unsafe { sys::llama_sampler_chain_default_params() };
        let smpl = unsafe { sys::llama_sampler_chain_init(sparams) };
        unsafe {
            sys::llama_sampler_chain_add(smpl, sys::llama_sampler_init_top_p(params.top_p, 1));
            sys::llama_sampler_chain_add(smpl, sys::llama_sampler_init_temp(params.temperature));
            sys::llama_sampler_chain_add(smpl, sys::llama_sampler_init_dist(params.seed));
        }
        smpl
    }

    /// Continuous batching (P5.1) : un seul `llama_decode` par pas de token
    /// pour jusqu'à `n_seq_max` séquences. `on_delta(i, piece)` pour le job `i`.
    pub fn generate_batch(
        &mut self,
        items: &[BatchItem],
        on_delta: impl FnMut(usize, &str) -> bool,
    ) -> Vec<Result<GenStats, LlamaError>> {
        self.generate_batch_admit(items, || None, |_| {}, on_delta)
    }

    /// Comme [`generate_batch`], mais `admit()` est appelé entre les pas de
    /// decode : un job chat/agent arrivé en cours de génération peut rejoindre
    /// le même `llama_decode` (parallélisme réel sur le GPU).
    ///
    /// Si un job admis échoue avant d'occuper un slot (tokenize / prompt trop
    /// long), `on_reject(err)` est appelé : l'appelant doit finaliser ce job
    /// hors du vecteur de résultats (qui ne contient que les items initiaux
    /// + les admits réellement démarrés).
    pub fn generate_batch_admit(
        &mut self,
        items: &[BatchItem],
        mut admit: impl FnMut() -> Option<BatchItem>,
        mut on_reject: impl FnMut(LlamaError),
        mut on_delta: impl FnMut(usize, &str) -> bool,
    ) -> Vec<Result<GenStats, LlamaError>> {
        let n = items.len();
        let mut out: Vec<Result<GenStats, LlamaError>> = (0..n)
            .map(|_| Err(LlamaError::Decode(-1)))
            .collect();
        if n == 0 {
            return out;
        }
        if n as u32 > self.n_seq_max {
            for slot in &mut out {
                *slot = Err(LlamaError::Decode(-2));
            }
            return out;
        }

        struct Slot {
            seq_id: sys::llama_seq_id,
            prompt_n: u32,
            generated: u32,
            max_tokens: u32,
            pos: sys::llama_pos,
            last: sys::llama_token,
            smpl: *mut sys::llama_sampler,
            abort: Arc<AtomicBool>,
            done: Option<StopReason>,
            ttft_ms: f64,
            t_start: Instant,
            t_done: Option<Instant>,
        }

        impl Slot {
            fn finish(&mut self, reason: StopReason) {
                if self.done.is_none() {
                    self.t_done = Some(Instant::now());
                    self.done = Some(reason);
                }
            }
        }

        let vocab = unsafe { sys::llama_model_get_vocab(self.model.ptr) };
        let mut slots: Vec<Slot> = Vec::with_capacity(self.n_seq_max as usize);
        let mut prompts: Vec<Vec<sys::llama_token>> = Vec::with_capacity(self.n_seq_max as usize);

        for (i, item) in items.iter().enumerate() {
            match self
                .render_prompt(&item.messages)
                .and_then(|p| self.tokenize(&p, false))
            {
                Ok(toks) => {
                    if toks.len() + item.params.max_tokens as usize + 8 > self.n_ctx_seq() as usize {
                        out[i] = Err(LlamaError::prompt_too_long(
                            toks.len(),
                            self.n_ctx_seq(),
                            item.params.max_tokens,
                        ));
                        prompts.push(Vec::new());
                        continue;
                    }
                    prompts.push(toks);
                    slots.push(Slot {
                        seq_id: i as sys::llama_seq_id,
                        prompt_n: 0,
                        generated: 0,
                        max_tokens: item.params.max_tokens,
                        pos: 0,
                        last: 0,
                        smpl: Self::make_sampler(&item.params),
                        abort: item.abort.clone(),
                        done: None,
                        ttft_ms: f64::MAX,
                        t_start: Instant::now(),
                        t_done: None,
                    });
                }
                Err(e) => {
                    out[i] = Err(e);
                    prompts.push(Vec::new());
                }
            }
        }

        if slots.is_empty() {
            return out;
        }

        unsafe {
            let mem = sys::llama_get_memory(self.ptr);
            sys::llama_memory_clear(mem, true);
        }

        let mut batch = unsafe {
            sys::llama_batch_init(self.n_batch as i32, 0, self.n_seq_max as i32)
        };

        let prefill_slot = |slf: &mut Self,
                            batch: &mut sys::llama_batch,
                            slots: &mut [Slot],
                            prompts: &[Vec<sys::llama_token>],
                            si: usize|
         -> Result<(), LlamaError> {
            let seq = slots[si].seq_id;
            let toks = &prompts[seq as usize];
            let mut off = 0usize;
            let mut last_logit = 0i32;
            while off < toks.len() {
                batch.n_tokens = 0;
                let take = (toks.len() - off).min(slf.n_batch as usize);
                for j in 0..take {
                    let is_last = off + j + 1 == toks.len();
                    let logit_i = batch.n_tokens;
                    unsafe {
                        Self::batch_add(
                            batch,
                            toks[off + j],
                            (off + j) as sys::llama_pos,
                            seq,
                            is_last,
                        );
                    }
                    if is_last {
                        last_logit = logit_i;
                    }
                }
                let rc = unsafe { sys::llama_decode(slf.ptr, *batch) };
                if rc != 0 {
                    return Err(LlamaError::Decode(rc));
                }
                off += take;
            }
            slots[si].prompt_n = toks.len() as u32;
            slots[si].pos = toks.len() as sys::llama_pos;
            slots[si].last =
                unsafe { sys::llama_sampler_sample(slots[si].smpl, slf.ptr, last_logit) };
            Ok(())
        };

        // Prefill initial (toutes les séquences).
        for si in 0..slots.len() {
            if let Err(e) = prefill_slot(self, &mut batch, &mut slots, &prompts, si) {
                for slot in &mut slots {
                    if slot.done.is_none() {
                        out[slot.seq_id as usize] = Err(e.clone());
                        slot.finish(StopReason::Aborted);
                    }
                }
                for slot in &slots {
                    unsafe { sys::llama_sampler_free(slot.smpl) };
                }
                unsafe { sys::llama_batch_free(batch) };
                return out;
            }
        }

        let try_admit = |slf: &mut Self,
                         batch: &mut sys::llama_batch,
                         slots: &mut Vec<Slot>,
                         prompts: &mut Vec<Vec<sys::llama_token>>,
                         out: &mut Vec<Result<GenStats, LlamaError>>,
                         admit: &mut dyn FnMut() -> Option<BatchItem>,
                         on_reject: &mut dyn FnMut(LlamaError)| {
            // Capacité GPU = nombre de slots (y compris terminés : pas de
            // réutilisation de seq_id / KV dans ce tour).
            while (slots.len() as u32) < slf.n_seq_max {
                let Some(item) = admit() else {
                    break;
                };
                let idx = out.len();
                if idx as u32 >= slf.n_seq_max {
                    on_reject(LlamaError::Decode(-2));
                    break;
                }
                match slf
                    .render_prompt(&item.messages)
                    .and_then(|p| slf.tokenize(&p, false))
                {
                    Ok(toks)
                        if toks.len() + item.params.max_tokens as usize + 8
                            <= slf.n_ctx_seq() as usize =>
                    {
                        out.push(Err(LlamaError::Decode(-1)));
                        prompts.push(toks);
                        slots.push(Slot {
                            seq_id: idx as sys::llama_seq_id,
                            prompt_n: 0,
                            generated: 0,
                            max_tokens: item.params.max_tokens,
                            pos: 0,
                            last: 0,
                            smpl: Self::make_sampler(&item.params),
                            abort: item.abort.clone(),
                            done: None,
                            ttft_ms: f64::MAX,
                            t_start: Instant::now(),
                            t_done: None,
                        });
                        let si = slots.len() - 1;
                        if let Err(e) = prefill_slot(slf, batch, slots, prompts, si) {
                            out[idx] = Err(e);
                            slots[si].finish(StopReason::Aborted);
                        } else {
                            eprintln!(
                                "[llama] admit seq={idx} ({} actifs)",
                                slots.iter().filter(|s| s.done.is_none()).count()
                            );
                        }
                    }
                    Ok(toks) => {
                        on_reject(LlamaError::prompt_too_long(
                            toks.len(),
                            slf.n_ctx_seq(),
                            item.params.max_tokens,
                        ));
                    }
                    Err(e) => {
                        on_reject(e);
                    }
                }
            }
        };

        try_admit(
            self,
            &mut batch,
            &mut slots,
            &mut prompts,
            &mut out,
            &mut admit,
            &mut on_reject,
        );

        loop {
            let mut any_active = false;
            for slot in &mut slots {
                if slot.done.is_some() {
                    continue;
                }
                if slot.abort.load(Ordering::SeqCst) {
                    slot.finish(StopReason::Aborted);
                    continue;
                }
                if unsafe { sys::llama_vocab_is_eog(vocab, slot.last) } {
                    slot.finish(StopReason::Eog);
                    continue;
                }
                if slot.generated >= slot.max_tokens {
                    slot.finish(StopReason::MaxTokens);
                    continue;
                }
                if slot.ttft_ms == f64::MAX {
                    slot.ttft_ms = slot.t_start.elapsed().as_secs_f64() * 1000.0;
                }
                let piece = self.token_to_piece(slot.last);
                if !piece.is_empty() && !on_delta(slot.seq_id as usize, &piece) {
                    slot.finish(StopReason::Aborted);
                    continue;
                }
                slot.generated += 1;
                any_active = true;
            }

            try_admit(
                self,
                &mut batch,
                &mut slots,
                &mut prompts,
                &mut out,
                &mut admit,
                &mut on_reject,
            );

            // Un slot tout juste admis a déjà un `last` (prefill) mais
            // generated==0 : le prochain tour émettra son premier delta.
            if !any_active && slots.iter().all(|s| s.done.is_some()) {
                break;
            }
            if !any_active {
                // Seulement des slots admis ce tour — continuer pour émettre.
                if slots.iter().any(|s| s.done.is_none()) {
                    continue;
                }
                break;
            }

            batch.n_tokens = 0;
            let mut order: Vec<usize> = Vec::new();
            for (k, slot) in slots.iter().enumerate() {
                if slot.done.is_some() {
                    continue;
                }
                unsafe {
                    Self::batch_add(&mut batch, slot.last, slot.pos, slot.seq_id, true);
                }
                order.push(k);
            }
            if order.is_empty() {
                break;
            }
            let rc = unsafe { sys::llama_decode(self.ptr, batch) };
            if rc != 0 {
                for k in order {
                    if slots[k].done.is_none() {
                        out[slots[k].seq_id as usize] = Err(LlamaError::Decode(rc));
                        slots[k].finish(StopReason::Aborted);
                    }
                }
                break;
            }
            for (logit_i, &k) in order.iter().enumerate() {
                let slot = &mut slots[k];
                slot.last =
                    unsafe { sys::llama_sampler_sample(slot.smpl, self.ptr, logit_i as i32) };
                slot.pos += 1;
            }
        }

        for slot in slots {
            let idx = slot.seq_id as usize;
            if out[idx].is_err() && matches!(out[idx], Err(LlamaError::Decode(-1))) {
                let total = slot
                    .t_done
                    .unwrap_or_else(Instant::now)
                    .saturating_duration_since(slot.t_start)
                    .as_secs_f64();
                let ttft = if slot.ttft_ms == f64::MAX {
                    total * 1000.0
                } else {
                    slot.ttft_ms
                };
                let decode_s = (total - ttft / 1000.0).max(1e-6);
                out[idx] = Ok(GenStats {
                    prompt_tokens: slot.prompt_n,
                    generated_tokens: slot.generated,
                    ttft_ms: ttft,
                    tok_s: slot.generated as f64 / decode_s,
                    stopped: slot.done.unwrap_or(StopReason::Eog),
                });
            }
            unsafe { sys::llama_sampler_free(slot.smpl) };
        }
        unsafe { sys::llama_batch_free(batch) };
        out
    }

    /// Calcule l'embedding d'un texte (contexte créé avec `embeddings=true`).
    ///
    /// Retourne le vecteur poolé (mean) normalisé L2, dimension
    /// `n_embd_out` du modèle.
    ///
    /// Prefill découpé par `min(n_batch, n_ubatch)` : `llama_batch` assert si
    /// `n > n_batch`, et `LLAMA_POOLING_TYPE_MEAN` est **local à l'ubatch**
    /// (écrase `embd_seq`). On accumule donc une moyenne pondérée par chunk.
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>, LlamaError> {
        let mut tokens = self.tokenize(text, true)?;
        if tokens.is_empty() {
            return Err(LlamaError::Tokenize);
        }
        if tokens.len() as u32 > self.n_ctx {
            tokens.truncate(self.n_ctx as usize);
        }
        let n = tokens.len();
        unsafe {
            let mem = sys::llama_get_memory(self.ptr);
            sys::llama_memory_clear(mem, true);
        }
        let dim = unsafe { sys::llama_model_n_embd_out(self.model.ptr) } as usize;
        let chunk = (unsafe { sys::llama_n_ubatch(self.ptr) })
            .min(self.n_batch)
            .max(1) as usize;
        let mut acc = vec![0.0f32; dim];
        let mut batch = unsafe { sys::llama_batch_init(chunk as i32, 0, 1) };
        let mut off = 0usize;
        while off < n {
            batch.n_tokens = 0;
            let take = (n - off).min(chunk);
            for j in 0..take {
                unsafe {
                    Self::batch_add(
                        &mut batch,
                        tokens[off + j],
                        (off + j) as sys::llama_pos,
                        0,
                        true,
                    );
                }
            }
            let rc = unsafe { sys::llama_decode(self.ptr, batch) };
            if rc != 0 {
                unsafe { sys::llama_batch_free(batch) };
                return Err(LlamaError::Decode(rc));
            }
            let Some(pooled) = self.pooled_seq_embedding(dim) else {
                unsafe { sys::llama_batch_free(batch) };
                return Err(LlamaError::Decode(-1));
            };
            accumulate_pooled_chunk(&mut acc, &pooled, take);
            off += take;
        }
        unsafe { sys::llama_batch_free(batch) };
        Ok(finish_mean_pool(acc, n))
    }

    /// Embedding poolé de seq 0 après le dernier decode (MEAN écrit `embd_seq`).
    fn pooled_seq_embedding(&self, dim: usize) -> Option<Vec<f32>> {
        let ptr = unsafe { sys::llama_get_embeddings_seq(self.ptr, 0) };
        let ptr = if ptr.is_null() {
            unsafe { sys::llama_get_embeddings(self.ptr) }
        } else {
            ptr
        };
        if ptr.is_null() {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(ptr, dim) }.to_vec())
    }

    /// Dimension des embeddings du modèle.
    pub fn embed_dim(&self) -> usize {
        unsafe { sys::llama_model_n_embd_out(self.model.ptr) as usize }
    }
}

/// Qwen3/3.5 hybrid thinking: le chat template ouvre souvent un bloc `<think>`
/// par défaut. Sans `enable_thinking=false` (non exposé par l'API C llama),
/// on préremplit une fermeture vide pour forcer la réponse utile.
fn suppress_hybrid_thinking(prompt: &str, template: &str) -> String {
    let tmpl_l = template.to_ascii_lowercase();
    let hybrid = tmpl_l.contains("enable_thinking")
        || tmpl_l.contains("<think>")
        || tmpl_l.contains("</think>");
    if !hybrid {
        return prompt.to_string();
    }
    let trimmed = prompt.trim_end();
    // Déjà prérempli / tour assistant déjà clos
    if trimmed.contains("</think>") {
        return prompt.to_string();
    }
    let open = "<think>";
    if trimmed.ends_with(open) || trimmed.ends_with("<think>\n") {
        return format!("{trimmed}\n</think>\n\n");
    }
    format!("{trimmed}{open}\n\n</think>\n\n")
}

/// Ajoute `n_tokens * mean_chunk` (MEAN llama.cpp = moyenne du chunk courant).
fn accumulate_pooled_chunk(acc: &mut [f32], chunk: &[f32], n_tokens: usize) {
    let w = n_tokens as f32;
    for (a, &x) in acc.iter_mut().zip(chunk.iter()) {
        *a += x * w;
    }
}

fn finish_mean_pool(mut acc: Vec<f32>, n_tokens: usize) -> Vec<f32> {
    if n_tokens > 0 {
        let inv = 1.0 / n_tokens as f32;
        for a in &mut acc {
            *a *= inv;
        }
    }
    l2_normalize(acc)
}

/// Normalisation L2 (pour similarité cosinus).
fn l2_normalize(v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.into_iter().map(|x| x / norm).collect()
    } else {
        v
    }
}

impl Drop for LlamaContext {
    fn drop(&mut self) {
        unsafe { sys::llama_free(self.ptr) };
    }
}

#[cfg(test)]
mod tests {
    use super::{accumulate_pooled_chunk, finish_mean_pool, l2_normalize};

    #[test]
    fn split_mean_matches_full_sequence_mean() {
        // Two decode chunks: 3 tokens then 1 — last-chunk-only would keep [4,4].
        let chunk_a = vec![1.0f32, 0.0];
        let chunk_b = vec![4.0f32, 4.0];
        let mut acc = vec![0.0f32; 2];
        accumulate_pooled_chunk(&mut acc, &chunk_a, 3);
        accumulate_pooled_chunk(&mut acc, &chunk_b, 1);
        let got = finish_mean_pool(acc, 4);
        let want = l2_normalize(vec![1.75, 1.0]);
        assert_eq!(got, want);
    }

    #[test]
    fn single_chunk_mean_is_just_l2() {
        let chunk = vec![3.0f32, 4.0];
        let mut acc = vec![0.0f32; 2];
        accumulate_pooled_chunk(&mut acc, &chunk, 5);
        assert_eq!(finish_mean_pool(acc, 5), l2_normalize(chunk));
    }
}
