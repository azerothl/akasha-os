//! Codec : trames CBOR préfixées par longueur sur TCP.

use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

/// Type de transport d'une trame.
pub type Transport = Framed<TcpStream, LengthDelimitedCodec>;

/// Construit le transport framé d'une connexion TCP.
pub fn transport(stream: TcpStream) -> Transport {
    let codec = LengthDelimitedCodec::builder()
        .max_frame_length(64 * 1024 * 1024)
        .new_codec();
    Framed::new(stream, codec)
}
