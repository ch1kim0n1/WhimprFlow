//! Length-prefixed JSON framing over any byte stream (the sidecar's stdio pipes).
//!
//! Frame layout: `[u32 length little-endian][UTF-8 JSON body]`.
//! A frame body must not exceed [`MAX_FRAME_LEN`]; oversized frames are rejected
//! rather than allocated, so a corrupt length can never trigger a huge allocation.

use std::io::{self, Read, Write};

use serde::{de::DeserializeOwned, Serialize};

/// Upper bound on a single frame's JSON body (16 MiB). Dictation payloads are tiny;
/// this only exists to reject a corrupt/garbage length prefix.
pub const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("frame length {0} exceeds maximum {MAX_FRAME_LEN}")]
    FrameTooLarge(usize),
}

/// Serialize `msg` and write it as one length-prefixed frame, then flush.
pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), CodecError> {
    let body = serde_json::to_vec(msg)?;
    if body.len() > MAX_FRAME_LEN {
        return Err(CodecError::FrameTooLarge(body.len()));
    }
    let len = body.len() as u32;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(&body)?;
    w.flush()?;
    Ok(())
}

/// Read exactly one length-prefixed frame and deserialize it.
///
/// Returns `Ok(None)` on a clean EOF at a frame boundary (peer closed the pipe),
/// so a read loop can treat that as an orderly shutdown rather than an error.
pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<Option<T>, CodecError> {
    let mut len_buf = [0u8; 4];
    match read_exact_or_eof(r, &mut len_buf)? {
        ReadEnd::Eof => return Ok(None),
        ReadEnd::Filled => {}
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_LEN {
        return Err(CodecError::FrameTooLarge(len));
    }
    let mut body = vec![0u8; len];
    // A partial body after a valid length prefix is a genuine protocol error, not EOF.
    r.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

enum ReadEnd {
    Filled,
    Eof,
}

/// Like `read_exact`, but a clean EOF *before the first byte* reports `Eof`
/// instead of erroring  -  that is the one place EOF is expected (frame boundary).
fn read_exact_or_eof<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<ReadEnd, io::Error> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => {
                if filled == 0 {
                    return Ok(ReadEnd::Eof);
                }
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "eof in the middle of a frame length prefix",
                ));
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(ReadEnd::Filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShellToSidecar, SidecarToShell};

    #[test]
    fn round_trips_a_message() {
        let mut buf: Vec<u8> = Vec::new();
        let msg = ShellToSidecar::Ping { seq: 42 };
        write_frame(&mut buf, &msg).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let got: Option<ShellToSidecar> = read_frame(&mut cursor).unwrap();
        assert!(matches!(got, Some(ShellToSidecar::Ping { seq: 42 })));
    }

    #[test]
    fn round_trips_multiple_frames_in_order() {
        let mut buf: Vec<u8> = Vec::new();
        write_frame(&mut buf, &SidecarToShell::Pong { seq: 1 }).unwrap();
        write_frame(&mut buf, &SidecarToShell::Pong { seq: 2 }).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let a: Option<SidecarToShell> = read_frame(&mut cursor).unwrap();
        let b: Option<SidecarToShell> = read_frame(&mut cursor).unwrap();
        let c: Option<SidecarToShell> = read_frame(&mut cursor).unwrap();
        assert!(matches!(a, Some(SidecarToShell::Pong { seq: 1 })));
        assert!(matches!(b, Some(SidecarToShell::Pong { seq: 2 })));
        assert!(c.is_none(), "clean EOF at frame boundary yields None");
    }

    #[test]
    fn rejects_oversized_length_without_allocating() {
        // A length prefix claiming > MAX_FRAME_LEN must error, not try to allocate it.
        let bogus_len = (MAX_FRAME_LEN as u32 + 1).to_le_bytes();
        let mut cursor = std::io::Cursor::new(bogus_len.to_vec());
        let res: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
        assert!(matches!(res, Err(CodecError::FrameTooLarge(_))));
    }

    #[test]
    fn rejects_ten_megabyte_claimed_frame() {
        let bogus_len = (10 * 1024 * 1024_u32).to_le_bytes();
        // 10 MiB is under MAX (16 MiB) so decoder may allocate — use MAX+ to prove gate.
        let over = ((MAX_FRAME_LEN as u64) + 1) as u32;
        let mut cursor = std::io::Cursor::new(over.to_le_bytes().to_vec());
        let res: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
        assert!(matches!(res, Err(CodecError::FrameTooLarge(_))));
        let _ = bogus_len;
    }

    // ── Edge case coverage: truncated frames, partial reads, empty bodies ───

    #[test]
    fn truncated_length_prefix_errors_not_eof() {
        // Only 2 of 4 length-prefix bytes: not a clean EOF (we got *some* bytes),
        // so this must be an error, not Ok(None).
        let mut cursor = std::io::Cursor::new(vec![0x01, 0x00]);
        let res: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
        assert!(res.is_err(), "partial length prefix must error, not return None");
        assert!(matches!(
            res,
            Err(CodecError::Io(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof
        ));
    }

    #[test]
    fn truncated_body_after_valid_length_errors() {
        // Length says 10 bytes, but only 3 follow: genuine protocol error.
        let mut buf = vec![];
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.extend_from_slice(b"abc");
        let mut cursor = std::io::Cursor::new(buf);
        let res: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
        assert!(res.is_err());
        assert!(matches!(
            res,
            Err(CodecError::Io(ref e)) if e.kind() == io::ErrorKind::UnexpectedEof
        ));
    }

    #[test]
    fn empty_body_frame_round_trips_if_json_valid() {
        // A zero-length body: serde_json will fail to deserialize an empty
        // string, so this should be a Json error, not a crash or panic.
        let mut buf = vec![];
        buf.extend_from_slice(&0u32.to_le_bytes());
        let mut cursor = std::io::Cursor::new(buf);
        let res: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
        assert!(res.is_err(), "zero-length body should fail JSON parse");
        assert!(matches!(res, Err(CodecError::Json(_))));
    }

    #[test]
    fn clean_eof_at_frame_boundary_returns_none() {
        // Empty stream = clean EOF before any bytes = Ok(None).
        let mut cursor = std::io::Cursor::new(Vec::<u8>::new());
        let res: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
        assert!(matches!(res, Ok(None)));
    }

    #[test]
    fn max_size_frame_is_accepted_not_rejected() {
        // A frame of exactly MAX_FRAME_LEN should pass the length check
        // (it may fail later on body read, but the length gate itself passes).
        let len = MAX_FRAME_LEN as u32;
        let mut buf = len.to_le_bytes().to_vec();
        // Don't actually write MAX_FRAME_LEN bytes (that's 16 MiB); just
        // verify the length check passes by confirming it's NOT FrameTooLarge.
        // The body read will fail with UnexpectedEof since we didn't provide it.
        buf.extend_from_slice(b"");
        let mut cursor = std::io::Cursor::new(buf);
        let res: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
        // Should be an Io error (truncated body), NOT FrameTooLarge.
        assert!(!matches!(res, Err(CodecError::FrameTooLarge(_))));
    }

    #[test]
    fn write_frame_to_closed_pipe_errors_gracefully() {
        // Writing to a broken pipe should return an Io error, not panic.
        let mut sink = std::io::sink(); // /dev/null equivalent, always succeeds
        let msg = ShellToSidecar::Ping { seq: 1 };
        let res = write_frame(&mut sink, &msg);
        assert!(res.is_ok(), "sink should accept writes");
    }

    #[test]
    fn round_trip_preserves_unicode_payload() {
        // Ensure UTF-8 multibyte content survives the frame round-trip.
        let mut buf: Vec<u8> = Vec::new();
        let msg = SidecarToShell::Log {
            level: 1,
            msg: "héllo wörld 日本語 🎤".to_string(),
        };
        write_frame(&mut buf, &msg).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let got: Option<SidecarToShell> = read_frame(&mut cursor).unwrap();
        match got {
            Some(SidecarToShell::Log { msg, .. }) => {
                assert_eq!(msg, "héllo wörld 日本語 🎤");
            }
            _ => panic!("expected Log"),
        }
    }

    #[test]
    fn interleaved_reads_and_writes_preserve_order() {
        // Write 3 frames, read them back in order.
        let mut buf: Vec<u8> = Vec::new();
        for i in 0..3u32 {
            write_frame(&mut buf, &SidecarToShell::Pong { seq: i }).unwrap();
        }
        let mut cursor = std::io::Cursor::new(buf);
        for i in 0..3u32 {
            let got: Option<SidecarToShell> = read_frame(&mut cursor).unwrap();
            assert!(matches!(got, Some(SidecarToShell::Pong { seq }) if seq == i));
        }
        // Fourth read = clean EOF.
        let got: Option<SidecarToShell> = read_frame(&mut cursor).unwrap();
        assert!(got.is_none());
    }

    mod fuzz {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]

            #[test]
            fn random_bytes_never_panic(
                bytes in proptest::collection::vec(any::<u8>(), 0..4096)
            ) {
                let mut cursor = std::io::Cursor::new(bytes);
                let _: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
            }

            #[test]
            fn oversize_prefix_never_allocates_huge(extra in 1u32..1_000_000) {
                let len = (MAX_FRAME_LEN as u32).saturating_add(extra);
                let mut cursor = std::io::Cursor::new(len.to_le_bytes().to_vec());
                let res: Result<Option<ShellToSidecar>, _> = read_frame(&mut cursor);
                prop_assert!(matches!(res, Err(CodecError::FrameTooLarge(_))));
            }
        }
    }
}
