//! Own process supervision independently of rmcp's process-wrap 9 transport.
//! rmcp still handles framing and protocol; the fixed supervisor owns the tree.
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use rmcp::{
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{async_rw::AsyncRwTransport, Transport},
    RoleClient,
};
use std::{future::Future, io, process::Stdio, time::Duration};
use tokio::process::{ChildStdin, ChildStdout, Command};

pub(super) struct SupervisedStdio {
    io: AsyncRwTransport<RoleClient, ChildStdout, ChildStdin>,
    child: Option<Box<dyn ChildWrapper>>,
}

pub(super) fn spawn(mut command: Command) -> io::Result<SupervisedStdio> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut wrapped = CommandWrap::from(command);
    #[cfg(unix)]
    wrapped.wrap(process_wrap::tokio::ProcessGroup::leader());
    #[cfg(windows)]
    crate::background::windows_job(&mut wrapped);
    wrapped.wrap(KillOnDrop);
    let mut child = wrapped.spawn()?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| io::Error::other("MCP stdout unavailable"))?;
    let stdin = child
        .stdin()
        .take()
        .ok_or_else(|| io::Error::other("MCP stdin unavailable"))?;
    Ok(SupervisedStdio {
        io: AsyncRwTransport::new(stdout, stdin),
        child: Some(child),
    })
}

impl Transport<RoleClient> for SupervisedStdio {
    type Error = io::Error;

    fn send(
        &mut self,
        message: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = io::Result<()>> + Send + 'static {
        self.io.send(message)
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        self.io.receive().await
    }

    async fn close(&mut self) -> io::Result<()> {
        self.io.close().await?;
        if let Some(child) = self.child.as_mut() {
            match tokio::time::timeout(Duration::from_secs(3), child.wait()).await {
                Ok(status) => {
                    status?;
                }
                Err(_) => {
                    child.start_kill()?;
                    tokio::time::timeout(Duration::from_secs(1), child.wait())
                        .await
                        .map_err(|_| {
                            io::Error::new(io::ErrorKind::TimedOut, "MCP shutdown timed out")
                        })??;
                }
            }
        }
        self.child.take();
        Ok(())
    }
}

impl Drop for SupervisedStdio {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Cancellation, failed handshake and runtime shutdown must also kill descendants.
            let _ = child.start_kill();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn stdio_preserves_protocol_messages_and_closes_cleanly() {
        let mut command = crate::background::tokio_command("node");
        command.args(["-e", r#"
            const lines = require('node:readline').createInterface({ input: process.stdin });
            lines.on('line', line => {
                const request = JSON.parse(line);
                process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result: {} }) + '\n');
            });
        "#]);
        let mut transport = spawn(command).unwrap();
        transport
            .send(
                serde_json::from_value(json!({ "jsonrpc": "2.0", "id": 17, "method": "ping" }))
                    .unwrap(),
            )
            .await
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(5), transport.receive())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            json!({ "jsonrpc": "2.0", "id": 17, "result": {} })
        );
        transport.close().await.unwrap();
        transport.close().await.unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn closing_and_dropping_stdio_terminate_the_process_tree() {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        };
        struct ObservedProcess(HANDLE);
        impl Drop for ObservedProcess {
            fn drop(&mut self) {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        for close in [false, true] {
            let mut command = crate::background::tokio_command("node");
            command.args(["-e", r#"
                const child = require('node:child_process').spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { windowsHide: true, stdio: 'ignore' });
                child.on('spawn', () => process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: 1, result: { parent: process.pid, descendant: child.pid } }) + '\n'));
                setInterval(() => {}, 1000);
            "#]);
            let mut transport = spawn(command).unwrap();
            let response = tokio::time::timeout(Duration::from_secs(5), transport.receive())
                .await
                .unwrap()
                .unwrap();
            let response = serde_json::to_value(response).unwrap();
            let processes = ["parent", "descendant"].map(|name| {
                let pid = response["result"][name].as_u64().unwrap() as u32;
                let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
                assert!(!handle.is_null());
                assert_eq!(unsafe { WaitForSingleObject(handle, 0) }, WAIT_TIMEOUT);
                ObservedProcess(handle)
            });
            if close {
                transport.close().await.unwrap();
            }
            drop(transport);
            tokio::time::timeout(Duration::from_secs(3), async {
                while processes
                    .iter()
                    .any(|process| unsafe { WaitForSingleObject(process.0, 0) } != WAIT_OBJECT_0)
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("MCP process tree survived transport shutdown");
        }
    }
}
