//! Small crate root so `mix` / `speech` can be exported without rewriting
//! the large `src/lib.rs` blob via GitHub Contents.
include!("lib.rs");
pub mod mix;
pub mod speech;
