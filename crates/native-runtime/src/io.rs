// SPDX-License-Identifier: MIT
use super::{Failure, MAX_FRAME};
use serde::Serialize;
use std::io::Write;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use zeroize::{Zeroize, Zeroizing};
const MAX_TOTAL: usize = 64 * MAX_FRAME;

pub(super) struct Input<R> {
    reader: R,
    buffer: Zeroizing<[u8; 8192]>,
    start: usize,
    end: usize,
    total: usize,
}
impl<R: AsyncRead + Unpin> Input<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Zeroizing::new([0; 8192]),
            start: 0,
            end: 0,
            total: 0,
        }
    }
    pub async fn frame(&mut self) -> Result<Zeroizing<Vec<u8>>, Failure> {
        let mut frame = Zeroizing::new(Vec::with_capacity(MAX_FRAME));
        loop {
            if self.start == self.end {
                self.buffer.zeroize();
                self.end = self
                    .reader
                    .read(&mut *self.buffer)
                    .await
                    .map_err(|_| Failure::Protocol)?;
                self.start = 0;
                if self.end == 0 {
                    return Err(Failure::Protocol);
                }
            }
            let available = &self.buffer[self.start..self.end];
            let count = available
                .iter()
                .position(|b| *b == b'\n')
                .map_or(available.len(), |i| i + 1);
            if count > MAX_FRAME - frame.len() || count > MAX_TOTAL - self.total {
                return Err(Failure::Protocol);
            }
            frame.extend_from_slice(&available[..count]);
            self.buffer[self.start..self.start + count].zeroize();
            self.start += count;
            self.total += count;
            if frame.last() == Some(&b'\n') {
                return Ok(frame);
            }
        }
    }
}
pub(super) struct Output<W> {
    writer: W,
    total: usize,
    pub contract: super::request::RequestContract,
    pub id: &'static str,
}
struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_FRAME - 1 - self.0.len() {
            return Err(std::io::Error::other("frame limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl<W: AsyncWrite + Unpin> Output<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            total: 0,
            contract: super::request::RequestContract::NegotiatedV1,
            id: "handshake",
        }
    }
    pub async fn send(&mut self, event: &impl Serialize) -> Result<(), Failure> {
        #[derive(Serialize)]
        struct Envelope<'a, T> {
            #[serde(skip_serializing_if = "Option::is_none")]
            protocol: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            protocol_version: Option<u32>,
            id: &'static str,
            #[serde(flatten)]
            event: &'a T,
        }
        let mut frame = Limited(Vec::new());
        serde_json::to_writer(
            &mut frame,
            &Envelope {
                protocol: match self.contract {
                    super::request::RequestContract::NegotiatedV1 => None,
                    super::request::RequestContract::LegacySetup => Some(3),
                    super::request::RequestContract::LegacyAuth => Some(4),
                },
                protocol_version: (self.contract == super::request::RequestContract::NegotiatedV1)
                    .then_some(1),
                id: self.id,
                event,
            },
        )
        .map_err(|_| Failure::Internal)?;
        frame.0.push(b'\n');
        if frame.0.len() > MAX_TOTAL - self.total {
            return Err(Failure::Internal);
        }
        self.total += frame.0.len();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            self.writer.write_all(&frame.0).await?;
            self.writer.flush().await
        })
        .await
        .map_err(|_| Failure::Unavailable)?
        .map_err(|_| Failure::Unavailable)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn frame_boundaries_truncation_and_transcript_limits_are_enforced() {
        let mut input = Input::new(&b"first\nsecond\n"[..]);
        assert_eq!(&*input.frame().await.unwrap(), b"first\n");
        assert_eq!(&*input.frame().await.unwrap(), b"second\n");
        assert!(input.frame().await.is_err());
        assert!(Input::new(&b"unterminated"[..]).frame().await.is_err());
        let mut exact = vec![b' '; MAX_FRAME];
        exact[MAX_FRAME - 1] = b'\n';
        assert_eq!(
            Input::new(exact.as_slice()).frame().await.unwrap().len(),
            MAX_FRAME
        );
        exact.insert(0, b' ');
        assert!(Input::new(exact.as_slice()).frame().await.is_err());
        let mut input = Input::new(&b"x\n"[..]);
        input.total = MAX_TOTAL - 1;
        assert!(input.frame().await.is_err());
    }
    #[tokio::test]
    async fn response_frame_and_total_budgets_fail_without_publishing_oversize_frame() {
        let mut bytes = Vec::new();
        let mut output = Output::new(&mut bytes);
        assert!(
            output
                .send(&serde_json::json!({"event":"error","code":"x".repeat(MAX_FRAME)}))
                .await
                .is_err()
        );
        assert_eq!(output.total, 0);
        output.total = MAX_TOTAL - 1;
        assert!(
            output
                .send(&serde_json::json!({"event":"cancelled"}))
                .await
                .is_err()
        );
        assert!(bytes.is_empty());
    }
    #[tokio::test(start_paused = true)]
    async fn stalled_stdout_has_a_fixed_write_deadline() {
        let (writer, _reader) = tokio::io::duplex(1);
        let mut output = Output::new(writer);
        let started = tokio::time::Instant::now();
        assert!(matches!(
            output.send(&serde_json::json!({"event":"cancelled"})).await,
            Err(Failure::Unavailable)
        ));
        assert_eq!(started.elapsed(), std::time::Duration::from_secs(5));
    }
}
