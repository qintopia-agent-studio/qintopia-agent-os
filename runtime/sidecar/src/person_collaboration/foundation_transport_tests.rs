//! Real Unix transport regression, isolated from global broker environment and SQL.
use anyhow::{bail, ensure, Result};
use serde_json::{json, Value};
use std::{os::unix::fs::PermissionsExt, path::Path, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::task::JoinHandle;

fn exit_description(exit: std::result::Result<Result<()>, tokio::task::JoinError>) -> String {
    match exit {
        Ok(Err(error)) => {
            let io = error.downcast_ref::<std::io::Error>();
            format!(
                "broker exited: {error}; io_kind={:?}; os_error={:?}",
                io.map(std::io::Error::kind),
                io.and_then(std::io::Error::raw_os_error)
            )
        }
        other => format!("unexpected broker exit: {other:?}"),
    }
}
async fn response(
    socket: &Path,
    request: &[u8],
    server: &mut JoinHandle<Result<()>>,
) -> Result<Value> {
    tokio::select! {
        biased;
        exit=&mut *server=>bail!("{}",exit_description(exit)),
        result=tokio::time::timeout(Duration::from_secs(3),async {
            let stream=tokio::net::UnixStream::connect(socket).await?;
            let (read,mut write)=stream.into_split();
            write.write_all(request).await?;
            let mut line=String::new();
            BufReader::new(read).read_line(&mut line).await?;
            Ok::<_,anyhow::Error>(serde_json::from_str(&line)?)
        })=>result?,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn foundation_broker_early_disconnect_preserves_listener_and_authentication() -> Result<()> {
    const CHILD: &str = "QINTOPIA_BROKER_DISCONNECT_TEST_CHILD";
    const COMPLETE: &str = "foundation_disconnect_regression_complete";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let output=std::process::Command::new(std::env::current_exe()?)
            .args(["person_collaboration::foundation_server_tests::transport::foundation_broker_early_disconnect_preserves_listener_and_authentication","--exact","--nocapture"])
            .env(CHILD,"1").output()?;
        ensure!(
            output.status.success(),
            "isolated transport regression failed\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        ensure!(
            String::from_utf8_lossy(&output.stderr).contains(COMPLETE),
            "isolated regression did not execute its completion checks"
        );
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        return Ok(());
    }
    let dir = tempfile::tempdir()?;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    // This probe observes the platform before invoking the production broker.
    let probe = dir.path().join("peer.sock");
    let listener = tokio::net::UnixListener::bind(&probe)?;
    drop(std::os::unix::net::UnixStream::connect(&probe)?);
    let (stream, _) = listener.accept().await?;
    match stream.peer_cred() {
        Ok(_) => eprintln!(
            "platform={} accept=ok closed_peer_cred=ok",
            std::env::consts::OS
        ),
        Err(error) => eprintln!(
            "platform={} accept=ok closed_peer_cred={error}; kind={:?}; os_error={:?}",
            std::env::consts::OS,
            error.kind(),
            error.raw_os_error()
        ),
    }
    drop(stream);
    drop(listener);
    let socket = dir.path().join("broker.sock");
    let token = "simulated-disconnect-model-token-0000";
    std::env::set_var("QINTOPIA_FOUNDATION_SOCKET", &socket);
    std::env::set_var("QINTOPIA_FOUNDATION_TOKEN", token);
    std::env::set_var(
        "QINTOPIA_FOUNDATION_HOST_TOKEN",
        "simulated-disconnect-host-token-00000",
    );
    std::env::set_var("QINTOPIA_FOUNDATION_GATEWAY_ID", "simulated-gateway");
    std::env::set_var("QINTOPIA_FOUNDATION_PROFILE", "erhua");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@127.0.0.1:1/unused")?;
    let store = crate::person_collaboration::Store {
        pool,
        tenant: "synthetic-collaboration-disconnect".into(),
        identity_namespace: "synthetic-collaboration-disconnect".into(),
        mode: super::super::store::StoreMode::Synthetic,
    };
    let mut server = tokio::spawn(super::super::foundation_server::broker(store));
    tokio::time::timeout(Duration::from_secs(3), async {
        while !socket.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let result:Result<()>=async{
        // No await between connect and drop. On this current-thread runtime the
        // broker cannot accept until the client has definitively closed.
        drop(std::os::unix::net::UnixStream::connect(&socket)?);
        let value=response(&socket,b"{}\n",&mut server).await?;
        ensure!(value["error"]["code"]=="invalid_request","invalid request was not rejected");
        eprintln!("early connect/drop followed by complete response: ok");
        for bytes in [b"{\"partial\":".as_slice(),b"{}\n".as_slice()] {
            use std::io::Write;
            let mut client=std::os::unix::net::UnixStream::connect(&socket)?;
            client.write_all(bytes)?;
            drop(client);
            let value=response(&socket,b"{}\n",&mut server).await?;
            ensure!(value["error"]["code"]=="invalid_request","listener lost after disconnected request");
        }
        let mut request=json!({"operation":"person_foundation_tool","schema_version":1,"agent":"wrong-profile","tool":"context","trusted_context":{"platform":"qiwe","chat_type":"direct","chat_id":"simulated-chat","sender_id":"simulated-resident","message_id":"simulated-message","gateway_id":"simulated-gateway"},"arguments":{},"token":token});
        for (supplied,expected) in [("wrong-token","authentication_required"),(token,"agent_tool_denied")] {
            request["token"]=json!(supplied);
            let mut bytes=serde_json::to_vec(&request)?;bytes.push(b'\n');
            let value=response(&socket,&bytes,&mut server).await?;
            ensure!(value["ok"]==false && value["error"]["code"]==expected,"authentication or dispatch contract changed");
        }
        ensure!(!server.is_finished(),"listener finished after subsequent requests");
        eprintln!("partial request, unread reply and subsequent authentication/dispatch: ok");
        eprintln!("{COMPLETE}");
        Ok(())
    }.await;
    if !server.is_finished() {
        server.abort();
        let _ = server.await;
    }
    result
}
