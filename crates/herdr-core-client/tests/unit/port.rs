use domain::ports::ControlPort;
use herdr_core_client::ClientCore;
use herdr_core_client::disposition;
use herdr_core_client::port::PortCore;
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

static CTR: AtomicU64 = AtomicU64::new(100);

fn socket_temp() -> PathBuf {
    let n = CTR.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("hcc-port-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("t.sock")
}

fn serve_once(sock: PathBuf, status_state: Option<String>) {
    thread::spawn(move || {
        let _ = std::fs::remove_file(&sock);
        let listener = UnixListener::bind(&sock).unwrap();
        // snapshot fait hello + terminal.list + doc.get = 3 connexions
        for _ in 0..3 {
            let (mut stream, _) = match listener.accept() {
                Ok(x) => x,
                Err(_) => break,
            };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                continue;
            }
            let req: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
            let method = req
                .get("method")
                .and_then(serde_json::Value::as_str)
                .unwrap();
            let id = req.get("id").cloned().unwrap_or(serde_json::Value::Null);
            let result = match method {
                "hello" => json!({"generation":1,"version":"0.1.17","features":[],"methods":[]}),
                "terminal.list" => {
                    let status = match status_state {
                        Some(ref s) => json!({"app":"opencode","state":s}),
                        None => serde_json::Value::Null,
                    };
                    json!({"terminals":[{"address":"t1","name":"x","argv":["opencode"],"cwd":"/tmp","state":"live","status":status,"turn":3,"title":"","labels":{},"parent":null,"foreground":{"pid":1234,"name":"opencode","argv":["opencode"],"cwd":"/tmp"}}]})
                }
                "doc.get" => json!({"value":null,"version":0}),
                _ => json!({}),
            };
            let resp = json!({"jsonrpc":"2.0","id":id,"result":result});
            let mut out = serde_json::to_string(&resp).unwrap();
            out.push('\n');
            let _ = stream.write_all(out.as_bytes());
            let _ = stream.flush();
        }
        thread::sleep(Duration::from_millis(100));
    });
    thread::sleep(Duration::from_millis(50));
}

#[test]
fn snapshot_mappe_status_blocked() {
    let sock = socket_temp();
    serve_once(sock.clone(), Some("blocked".to_string()));
    let client = ClientCore::nouveau(&sock);
    let port = PortCore::nouveau(client);
    let snap = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(port.snapshot())
        .expect("snapshot");
    assert_eq!(snap.agents.len(), 1);
    assert_eq!(snap.agents[0].status, domain::status::AgentStatus::Blocked);
    assert_eq!(snap.agents[0].foreground_pid, Some(1234));
}

#[test]
fn snapshot_mappe_status_done() {
    let sock = socket_temp();
    serve_once(sock.clone(), Some("done".to_string()));
    let client = ClientCore::nouveau(&sock);
    let port = PortCore::nouveau(client);
    let snap = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(port.snapshot())
        .expect("snapshot");
    assert_eq!(snap.agents[0].status, domain::status::AgentStatus::Done);
}

#[test]
fn snapshot_sans_status_n_est_pas_agent() {
    let sock = socket_temp();
    serve_once(sock.clone(), None);
    let client = ClientCore::nouveau(&sock);
    let port = PortCore::nouveau(client);
    let snap = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(port.snapshot())
        .expect("snapshot");
    assert!(snap.agents.is_empty());
    // le terminal hors disposition est quand même un pane
    assert_eq!(snap.panes.len(), 1);
}
