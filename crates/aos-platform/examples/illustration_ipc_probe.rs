//! Isolated desktop probe. Args: BUS SUBJECT CONSTRUCTION_FILE FRAME_SUBJECT [SESSION_ID [--refine CORRECTION]].
//! Without SESSION_ID, create/open a fixture. With it, generate and observe real IPC.
use aos_ipc::BusClient;
use aos_proto::*;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if !(5..=6).contains(&args.len()) && !(args.len() == 8 && args[6] == "--refine")
        && !(args.len() == 7 && args[6] == "--release") {
        return Err("expected BUS SUBJECT CONSTRUCTION_FILE FRAME_SUBJECT [SESSION_ID [--refine CORRECTION]]".into());
    }
    let bus = BusClient::connect(&args[1], "illustration-benchmark").await?;
    let sid = if let Some(sid) = args.get(5) { sid.clone() } else {
        let meta: ChatSessionMeta = bus.call("chat.session.create", &ChatSessionCreateRequest {
            title: Some("Illustration · test IPC isolé".into()), model_id: None,
        }, vec![]).await?;
        meta.id
    };
    if args.len() == 7 {
        let doc: IllustrationDoc = bus.call("illust.lock.release", &IllustLockReleaseRequest {
            session_id: sid, holder: "human:benchmark".into(),
        }, vec![]).await?;
        println!("Probe lock released; selected image: {:?}", doc.last_png);
        return Ok(());
    }
    let _: IllustrationDoc = bus.call("illust.lock.acquire", &IllustLockAcquireRequest {
        session_id: sid.clone(), holder: "human:benchmark".into(),
        reason: "isolated desktop probe".into(), ttl_ms: Some(600_000),
    }, vec![]).await?;
    if args.len() == 5 {
        let _: IllustrationDoc = bus.call("illust.set_brief", &IllustSetBriefRequest {
            session_id: sid.clone(), holder: "human:benchmark".into(),
            brief: IllustrationBrief { subject: args[2].clone(), ..Default::default() },
        }, vec![]).await?;
        let _: ChatSessionMeta = bus.call("illust.set_open", &IllustSetOpenRequest {
            session_id: sid.clone(), open: true,
        }, vec![]).await?;
        println!("Fixture session: {sid}. Select it in the isolated UI, then rerun with this SESSION_ID.");
        let _: IllustrationDoc = bus.call("illust.lock.release", &IllustLockReleaseRequest {
            session_id: sid, holder: "human:benchmark".into(),
        }, vec![]).await?;
        return Ok(());
    }
    let started: IllustrationDoc = if args.len() == 8 {
        bus.call("illust.refine_image", &IllustRefineImageRequest {
            session_id: sid.clone(), holder: "human:benchmark".into(), correction: args[7].clone(),
        }, vec![]).await?
    } else { bus.call("illust.generate_image", &IllustGenerateImageRequest {
        session_id: sid.clone(), holder: "human:benchmark".into(),
        construction: std::fs::read_to_string(&args[3])?, frame_subject: Some(args[4].clone()), seed: Some(42), pose_reference_png: None,
    }, vec![]).await? };
    let start = Instant::now();
    let mut seen = started.pass_previews.len();
    loop {
        let response: IllustGetResponse = bus.call("illust.get", &IllustGetRequest {session_id: sid.clone()}, vec![]).await?;
        let doc = response.doc;
        let run = doc.image_run.as_ref().ok_or("missing run")?;
        if doc.pass_previews.len() != seen {
            seen = doc.pass_previews.len();
            println!("{:.1}s {:?}: {:?}", start.elapsed().as_secs_f32(), run.phase, doc.last_png);
        }
        match run.status {
            IllustrationImageStatus::NeedsReview => {
                println!("IPC complete; review still required. Session: {sid}");
                println!("{}", serde_json::to_string_pretty(run)?);
                let _: IllustrationDoc = bus.call("illust.lock.release", &IllustLockReleaseRequest {
                    session_id: sid.clone(), holder: "human:benchmark".into(),
                }, vec![]).await?;
                break;
            }
            IllustrationImageStatus::Failed => return Err(run.error.clone().unwrap_or_default().into()),
            IllustrationImageStatus::Running => {}
        }
        if start.elapsed() > Duration::from_secs(600) { return Err("probe deadline".into()); }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Ok(())
}
