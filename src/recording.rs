//! A client whose socket is a file: a Disk.
//!
//! The owner named it (2026-08-22): in Tron a program's identity disc holds everything it has
//! done and seen, and this file holds everything the world said - so a recording is a *Disk*
//! (`.disk`), and the program that reads Disks back, replays them and checks them is *Clu*.
//!
//! The state log the topology owes (TOPOLOGY.md § The protocol, "two logs on the server") is
//! not a second encoder: it is this library's own frames written to a file by a *recording* —
//! a server-held end with no peer, fed by the same per-subscriber send loop as every citizen —
//! and read back by a *replay* — a client-held end whose `poll` reads the file. A replay viewer
//! is therefore a spectator that opened a recording instead of dialling a world, and a
//! recording keeps working across simulation changes because it holds what was *said*, never
//! how it was computed. One encoder, one framing, one fingerprint check, in the one place they
//! cannot drift.
//!
//! The file opens with a header naming the wire it speaks and the world it was made in, because
//! a recording that cannot say which contract it carries is unreadable after the first protocol
//! change; then the frames, exactly as they went out, in order.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use crate::codec::{EncodeError, Message, decode};
use crate::protocol::PROTOCOL_VERSION;
use crate::transport::{FrameReader, TransportError, WriteBuffer, recorded_fingerprint};

/// The recording file's opening bytes: the format and its revision, so a file of another shape
/// refuses at the door rather than decoding as frames.
pub const RECORDING_MAGIC: [u8; 8] = *b"DISK\0\0\0\x01";

/// The recording header, in the order it is written: what the file speaks, where it began.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RecordingHeader {
    pub protocol_version: u32,
    pub fingerprint: [u8; 32],
    pub world_fingerprint: u64,
    pub start_tick: u64,
    pub nominal_dt_seconds: f32,
    pub start_unix_seconds: u64,
}

const HEADER_BYTES: usize = 8 + 4 + 32 + 8 + 8 + 4 + 8;

impl RecordingHeader {
    fn write_to(&self, sink: &mut impl Write) -> std::io::Result<()> {
        sink.write_all(&RECORDING_MAGIC)?;
        sink.write_all(&self.protocol_version.to_le_bytes())?;
        sink.write_all(&self.fingerprint)?;
        sink.write_all(&self.world_fingerprint.to_le_bytes())?;
        sink.write_all(&self.start_tick.to_le_bytes())?;
        sink.write_all(&self.nominal_dt_seconds.to_le_bytes())?;
        sink.write_all(&self.start_unix_seconds.to_le_bytes())
    }

    fn read_from(source: &mut impl Read) -> Result<RecordingHeader, TransportError> {
        let mut bytes = [0u8; HEADER_BYTES];
        let mut filled = 0;
        while filled < bytes.len() {
            match source.read(&mut bytes[filled..]) {
                Ok(0) => return Err(TransportError::Garbled("a recording shorter than its own header")),
                Ok(read) => filled += read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        if bytes[..8] != RECORDING_MAGIC {
            return Err(TransportError::Garbled("not a recording: the magic is wrong"));
        }
        let u32_at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        let u64_at = |at: usize| {
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&bytes[at..at + 8]);
            u64::from_le_bytes(raw)
        };
        let mut fingerprint = [0u8; 32];
        fingerprint.copy_from_slice(&bytes[12..44]);
        Ok(RecordingHeader {
            protocol_version: u32_at(8),
            fingerprint,
            world_fingerprint: u64_at(44),
            start_tick: u64_at(52),
            nominal_dt_seconds: f32::from_le_bytes([bytes[60], bytes[61], bytes[62], bytes[63]]),
            start_unix_seconds: u64_at(64),
        })
    }
}

/// The judgement every Disk path passes before a file is touched: it names a `.disk`, and it
/// never climbs - no `..` component - so a path handed across the ABI can reach only where the
/// caller's own working directory or absolute root says. The operator chooses the path; this
/// rule makes a typo a refusal rather than a file somewhere surprising.
fn judged_path(path: &Path) -> Result<&Path, TransportError> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("disk") {
        return Err(TransportError::Refused {
            reason: "link: a Disk is named with the .disk extension".to_string(),
        });
    }
    if path.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
        return Err(TransportError::Refused {
            reason: "link: a Disk path never climbs - no .. component".to_string(),
        });
    }
    Ok(path)
}

/// A server-held end with no peer: everything queued is written to the file on flush, exactly
/// as it would have gone down a socket.
pub struct Recorder {
    file: BufWriter<File>,
    writer: WriteBuffer,
    bytes_written: u64,
}

impl Recorder {
    /// Create the file and write its header. The fingerprint is this build's own; the world
    /// fingerprint and the start are the caller's, as a WELCOME's would be.
    pub fn create(path: &Path, world_fingerprint: u64, start_tick: u64, nominal_dt_seconds: f32, start_unix_seconds: u64) -> Result<Recorder, TransportError> {
        let mut file = BufWriter::new(File::create(judged_path(path)?)?);
        let header = RecordingHeader {
            protocol_version: PROTOCOL_VERSION,
            fingerprint: recorded_fingerprint(),
            world_fingerprint,
            start_tick,
            nominal_dt_seconds,
            start_unix_seconds,
        };
        header.write_to(&mut file)?;
        file.flush()?;
        Ok(Recorder {
            file,
            writer: WriteBuffer::new(),
            bytes_written: HEADER_BYTES as u64,
        })
    }

    /// Stage a message, refusing exactly what the codec refuses. A file has no peer, so no
    /// direction rule applies: the caller records what it chooses to.
    pub fn queue(&mut self, message: &Message) -> Result<(), EncodeError> {
        self.writer.queue(message)
    }

    /// Write everything queued. Always everything: a file never says "later".
    pub fn flush(&mut self) -> Result<bool, TransportError> {
        let before = self.writer.pending_bytes();
        let result = self.writer.flush_into(&mut self.file);
        // What actually left, error or not - a rotation reads this, and a short write that
        // failed still wrote.
        self.bytes_written += (before - self.writer.pending_bytes()) as u64;
        let done = result?;
        self.file.flush()?;
        Ok(done)
    }

    /// Bytes on disk so far, header included - what a rotation policy reads.
    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Write the farewell and close: a recording ends with BYE as a world does, so a replay
    /// reaching it knows the world ended rather than the file being cut.
    pub fn close(mut self) {
        let _ = self.writer.queue(&Message::Bye);
        let _ = self.writer.flush_into(&mut self.file);
        let _ = self.file.flush();
    }
}

/// A client-held end whose socket is a file: `poll` yields the recorded frames in order, and the
/// end of the file is the peer closing.
pub struct Replayer {
    file: BufReader<File>,
    reader: FrameReader,
    header: RecordingHeader,
}

impl Replayer {
    /// Open a recording and judge its header as a handshake is judged: the wrong magic is
    /// garbled, another contract is refused naming both versions, and another world is refused
    /// naming both fingerprints - the same words a server would use, because a replay viewer is
    /// a client and deserves the same refusals.
    pub fn open(path: &Path, world_fingerprint: u64) -> Result<Replayer, TransportError> {
        let mut file = BufReader::new(File::open(judged_path(path)?)?);
        let header = RecordingHeader::read_from(&mut file)?;
        if header.fingerprint != recorded_fingerprint() {
            return Err(TransportError::Refused {
                reason: if header.protocol_version == PROTOCOL_VERSION {
                    "link: this recording was made with a modified lnk_protocol.h at the same version - one of us carries a modified header".to_string()
                } else {
                    format!(
                        "link: this recording speaks protocol version {} and this build speaks version {PROTOCOL_VERSION}",
                        header.protocol_version
                    )
                },
            });
        }
        if header.world_fingerprint != world_fingerprint {
            return Err(TransportError::Refused {
                reason: format!(
                    "link: this recording was made in a different world - world fingerprint {:016X} there, {world_fingerprint:016X} here - replay it with the world it was made in",
                    header.world_fingerprint
                ),
            });
        }
        Ok(Replayer {
            file,
            reader: FrameReader::new(),
            header,
        })
    }

    #[must_use]
    pub fn header(&self) -> &RecordingHeader {
        &self.header
    }

    /// The next recorded frame, decoded and judged exactly as a socket's would be. The end of
    /// the file is [`TransportError::PeerClosed`]: the recording is over.
    pub fn poll(&mut self) -> Result<Option<Message>, TransportError> {
        match self.reader.pump(&mut self.file)? {
            None => Ok(None),
            Some((type_byte, payload)) => Ok(Some(decode(type_byte, &payload).map_err(TransportError::Frame)?)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{CreatureState, TickStateHeader, Welcome};

    fn scratch(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("link-disk-test-{name}-{}.disk", std::process::id()));
        path
    }

    #[test]
    fn a_recording_replays_what_was_said_in_order_and_ends_with_bye() {
        let path = scratch("roundtrip");
        let mut recorder = Recorder::create(&path, 0xABCD, 100, 0.031_25, 1_700_000_000).expect("create");
        recorder
            .queue(&Message::Welcome(Welcome {
                current_tick: 100,
                nominal_dt_seconds: 0.031_25,
                client_id: 0,
                world_fingerprint: 0xABCD,
            }))
            .expect("queue");
        recorder
            .queue(&Message::TickState {
                header: TickStateHeader {
                    tick: 101,
                    creature_count: 1,
                    reserved0: [0; 4],
                },
                states: vec![CreatureState {
                    creature_id: 7,
                    position: [1.0, 2.0, 3.0],
                    yaw: 0.5,
                    velocity: [0.0; 3],
                    yaw_rate: 0.0,
                    vocalisation: 0.0,
                    segment_count: 1,
                    segments: [crate::protocol::SegmentPose::default(); crate::protocol::TRAILING_SEGMENTS_MAX],
                }],
            })
            .expect("queue");
        assert!(recorder.flush().expect("flush"));
        assert!(recorder.bytes_written() > HEADER_BYTES as u64);
        recorder.close();

        let mut replay = Replayer::open(&path, 0xABCD).expect("open");
        assert_eq!(replay.header().start_tick, 100);
        assert_eq!(replay.header().protocol_version, PROTOCOL_VERSION);
        assert!(matches!(replay.poll(), Ok(Some(Message::Welcome(_)))));
        match replay.poll() {
            Ok(Some(Message::TickState { header, states })) => {
                assert_eq!(header.tick, 101);
                assert_eq!(states[0].creature_id, 7);
            }
            other => panic!("expected the tick, got {other:?}"),
        }
        assert!(matches!(replay.poll(), Ok(Some(Message::Bye))));
        assert!(matches!(replay.poll(), Err(TransportError::PeerClosed)), "the end of the file is the peer closing");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_disk_path_must_be_a_disk_and_must_not_climb() {
        let climbing = std::path::PathBuf::from("../somewhere/else.disk");
        assert!(matches!(Recorder::create(&climbing, 1, 0, 0.031_25, 0), Err(TransportError::Refused { .. })));
        assert!(matches!(Replayer::open(&climbing, 1), Err(TransportError::Refused { .. })));
        let mut not_a_disk = std::env::temp_dir();
        not_a_disk.push("link-not-a-disk.txt");
        assert!(matches!(Recorder::create(&not_a_disk, 1, 0, 0.031_25, 0), Err(TransportError::Refused { .. })));
        assert!(!not_a_disk.exists(), "a refused path is never touched");
    }

    #[test]
    fn another_world_and_a_wrong_magic_are_refused_in_words() {
        let path = scratch("refusals");
        Recorder::create(&path, 1, 0, 0.031_25, 0).expect("create").close();
        match Replayer::open(&path, 2) {
            Err(TransportError::Refused { reason }) => assert!(reason.contains("different world"), "{reason}"),
            other => panic!("expected a worded refusal, got {:?}", other.err()),
        }
        std::fs::write(&path, b"not a recording at all, and long enough to be read as a header........").expect("write");
        assert!(matches!(Replayer::open(&path, 1), Err(TransportError::Garbled(_))));
        std::fs::write(&path, b"short").expect("write");
        assert!(matches!(Replayer::open(&path, 1), Err(TransportError::Garbled(_))));
        let _ = std::fs::remove_file(&path);
    }
}
