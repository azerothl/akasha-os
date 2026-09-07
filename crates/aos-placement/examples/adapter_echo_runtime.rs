//! Runtime de référence pour tester le protocole adaptateur sans matériel.
//!
//! Usage:
//! `cargo run -p aos-placement --example adapter_echo_runtime -- npu 127.0.0.1:38471`
//! Ce processus n'effectue aucun calcul accéléré : il renvoie le tenseur reçu
//! et sert uniquement de fake backend pour les tests d'intégration.

use aos_placement::{
    AdapterExecutionPhase, AdapterHandshake, AdapterRpcMessage, BackendKind, Quantization,
    ADAPTER_PROTOCOL_VERSION, ADAPTER_RPC_MAX_FRAME_BYTES, ADAPTER_RPC_MAX_TENSOR_BYTES,
};
use std::env;
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    if matches!(args.next().as_deref(), Some("--help" | "-h")) {
        println!("usage: adapter_echo_runtime [npu|webgpu] [address]");
        println!("default: npu 127.0.0.1:38471");
        return Ok(());
    }
    let backend_arg = args.next();
    let backend = match backend_arg.as_deref() {
        Some("webgpu") => BackendKind::WebGpu,
        Some("npu") | None => BackendKind::Npu,
        Some(other) => return Err(format!("backend inconnu: {other} (npu|webgpu)").into()),
    };
    let address = args.next().unwrap_or_else(|| "127.0.0.1:38471".into());
    let listener = TcpListener::bind(&address).await?;
    println!("fake adapter {:?} en écoute sur tcp://{address}", backend);
    loop {
        let (stream, peer) = listener.accept().await?;
        tokio::spawn(handle(stream, peer.to_string(), backend));
    }
}

async fn handle(mut stream: TcpStream, peer: String, backend: BackendKind) {
    let result: io::Result<()> = async {
        let hello = read_message(&mut stream).await?;
        match hello {
            AdapterRpcMessage::Hello {
                protocol_version,
                backend: requested,
            } if protocol_version == ADAPTER_PROTOCOL_VERSION && requested == backend => {}
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "hello refusé",
                ))
            }
        }
        write_message(
            &mut stream,
            &AdapterRpcMessage::Handshake(AdapterHandshake {
                protocol_version: ADAPTER_PROTOCOL_VERSION,
                backend,
                device: format!("fake-{}", backend_name(backend)),
                memory_bytes: 1 << 30,
                supported_operations: vec!["gemm".into(), "gemv".into()],
                supported_quantizations: vec![
                    Quantization::F16,
                    Quantization::Q4,
                    Quantization::Q8,
                ],
            }),
        )
        .await?;
        loop {
            match read_message(&mut stream).await? {
                AdapterRpcMessage::Execute {
                    request_id,
                    phase,
                    operation,
                    tensor,
                    ..
                } if !tensor.is_empty() && tensor.len() <= ADAPTER_RPC_MAX_TENSOR_BYTES => {
                    let _ = (phase, operation);
                    write_message(
                        &mut stream,
                        &AdapterRpcMessage::Result {
                            request_id,
                            output: tensor,
                        },
                    )
                    .await?;
                }
                AdapterRpcMessage::Execute { request_id, .. } => {
                    write_message(
                        &mut stream,
                        &AdapterRpcMessage::Error {
                            request_id: Some(request_id),
                            message: "tenseur fake hors limites".into(),
                        },
                    )
                    .await?;
                }
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "message refusé")),
            }
        }
    }
    .await;
    if let Err(error) = result {
        eprintln!("fake adapter {peer}: {error}");
    }
}

fn backend_name(backend: BackendKind) -> &'static str {
    match backend {
        BackendKind::Npu => "npu",
        BackendKind::WebGpu => "webgpu",
        _ => "other",
    }
}

async fn write_message(stream: &mut TcpStream, message: &AdapterRpcMessage) -> io::Result<()> {
    let mut encoded = Vec::new();
    ciborium::into_writer(message, &mut encoded)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    if encoded.is_empty() || encoded.len() > ADAPTER_RPC_MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "trame hors limites",
        ));
    }
    stream
        .write_all(&(encoded.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(&encoded).await?;
    stream.flush().await
}

async fn read_message(stream: &mut TcpStream) -> io::Result<AdapterRpcMessage> {
    let mut length = [0u8; 4];
    stream.read_exact(&mut length).await?;
    let size = u32::from_be_bytes(length) as usize;
    if size == 0 || size > ADAPTER_RPC_MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trame hors limites",
        ));
    }
    let mut encoded = vec![0u8; size];
    stream.read_exact(&mut encoded).await?;
    ciborium::from_reader(encoded.as_slice())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))
}

#[allow(dead_code)]
fn _phase_name(phase: AdapterExecutionPhase) -> &'static str {
    match phase {
        AdapterExecutionPhase::Prefill => "prefill",
        AdapterExecutionPhase::Decode => "decode",
    }
}
