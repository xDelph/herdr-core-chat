use herdr_core_client::{ClientCore, ErreurTransport};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use std::sync::atomic::{AtomicU64, Ordering};
static CTR: AtomicU64 = AtomicU64::new(0);
fn socket_temp() -> PathBuf {
    let n = CTR.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("hcc-test-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("t.sock")
}

#[test]
fn hello_et_terminal_list_sur_core_simule() {
    let sock = socket_temp();
    let sock_srv = sock.clone();
    let handle = thread::spawn(move || {
        let _ = std::fs::remove_file(&sock_srv);
        let listener = UnixListener::bind(&sock_srv).unwrap();
        // accepter 2 connexions (hello + terminal.list)
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let req: Value = serde_json::from_str(line.trim()).unwrap();
            let method = req.get("method").and_then(Value::as_str).unwrap();
            let id = req.get("id").cloned().unwrap_or(Value::Null);
            let result = match method {
                "hello" => {
                    json!({"generation":1,"version":"0.1.17","features":["framed"],"methods":["hello","terminal.list"],"framed":false})
                }
                "terminal.list" => {
                    json!({"terminals":[{"address":"t1","name":"x","argv":["sleep","10"],"cwd":"/tmp","state":"live","status":null,"turn":0,"title":"","labels":{},"parent":null}]})
                }
                _ => json!({}),
            };
            let resp = json!({"jsonrpc":"2.0","id":id,"result":result});
            let mut out = serde_json::to_string(&resp).unwrap();
            out.push('\n');
            stream.write_all(out.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    });

    // laisser le serveur se lier
    thread::sleep(Duration::from_millis(50));

    let mut client = ClientCore::nouveau(&sock);
    let hello = client.hello().expect("hello");
    assert_eq!(hello.version, "0.1.17");
    let list = client.terminal_list().expect("terminal.list");
    assert_eq!(list.terminals.len(), 1);
    assert_eq!(list.terminals[0].address, "t1");
    assert_eq!(list.terminals[0].turn, 0);

    handle.join().unwrap();
    let _ = std::fs::remove_file(&sock);
}

#[test]
fn erreur_amont_est_propagee() {
    let sock = socket_temp();
    let sock_srv = sock.clone();
    let handle = thread::spawn(move || {
        let _ = std::fs::remove_file(&sock_srv);
        let listener = UnixListener::bind(&sock_srv).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let req: Value = serde_json::from_str(line.trim()).unwrap();
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let resp = json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":"boom","data":{"kind":"not_found"}}});
        let mut out = serde_json::to_string(&resp).unwrap();
        out.push('\n');
        stream.write_all(out.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    thread::sleep(Duration::from_millis(50));
    let mut client = ClientCore::nouveau(&sock);
    let err = client.server_status().unwrap_err();
    match err {
        ErreurTransport::AmontRefus { kind, .. } => assert_eq!(kind.as_deref(), Some("not_found")),
        other => panic!("attendu AmontRefus, eu {other}"),
    }
    handle.join().unwrap();
    let _ = std::fs::remove_file(&sock);
}
