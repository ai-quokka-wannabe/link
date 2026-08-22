//! The socket the codec's refusals guard.
//!
//! `std::net` TCP with `TCP_NODELAY` on both ends, one coalesced write per tick, and no threads:
//! each consumer owns its loop, and everything here is a state machine it turns. The handshake
//! is timeout-bounded and blocking — nothing else to do before the wire exists — and the framed
//! phase after it is non-blocking: [`Connection::poll`] returns what a socket has whole, never
//! waits, and hangs up loudly on anything the contract refuses.
//!
//! The reader enforces the frame rules **at header time**: three bytes in, the type and length
//! are judged by [`payload_rule`] and [`check_length`], and a hostile length ends the
//! connection before a single payload byte is read. The reader also never reads past the frame
//! it is assembling, so one frame's bytes can never smear into the next — simplicity bought
//! with a few extra reads per tick, which localhost does not notice.

use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::codec::{DecodeError, EncodeError, Message, check_length, decode, decode_frame_header, encode, max_frame_bytes, payload_rule};
use crate::protocol::{FRAME_HEADER_BYTES, Hello, MAGIC, MessageType, PROTOCOL_VERSION, Role, Welcome};

/// Why the transport gave up. Refusals carry words because a refusal that cannot be diagnosed
/// gets worked around instead of fixed.
#[derive(Debug)]
pub enum TransportError {
    /// The operating system's verdict, minus the two kinds the state machine consumes
    /// (`WouldBlock` becomes "no frame yet"; timeouts during the handshake become [`TransportError::HandshakeTimedOut`]).
    Io(ErrorKind),
    /// The peer sent something the contract refuses. The connection is not worth keeping: a
    /// peer that framed one message wrongly will frame the next wrongly too.
    Frame(DecodeError),
    /// The far end said no during the handshake, in words. The words are the server's refusal
    /// line, verbatim, for the log.
    Refused { reason: String },
    /// The handshake bytes were not the handshake — wrong magic, a non-WELCOME answer, a
    /// truncated exchange. The `&'static str` names the expectation that was violated.
    Garbled(&'static str),
    /// The other end went away, orderly or otherwise. A power cut sends no BYE.
    PeerClosed,
    /// The handshake's timeout elapsed. Windows reports this as `TimedOut` and Unix as
    /// `WouldBlock`; both normalise here so callers match one thing.
    HandshakeTimedOut,
    /// An ACTIONS frame arrived on a spectator-role connection: a protocol violation the
    /// library enforces itself, on the server end, so the rule cannot drift between consumers.
    /// The connection is shut before this is returned.
    ActionsFromSpectator,
    /// The outgoing buffer passed [`WRITE_BUFFER_LIMIT_BYTES`] — the peer is not reading, and
    /// an unbounded buffer would be an allocation the peer controls. The connection is over.
    WriteBufferOverflow,
}

/// The most bytes the outgoing buffer may hold before the connection is declared dead rather
/// than the buffer grown: a megabyte is hundreds of full-world ticks, so a peer this far behind
/// is not merely slow. The keepalive constants usually reap such a peer first; this is the
/// backstop that bounds memory even when they have not yet.
pub const WRITE_BUFFER_LIMIT_BYTES: usize = 1_048_576;

impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        TransportError::Io(error.kind())
    }
}

/// The SHA-256 the fingerprint tool recorded beside the header, parsed from the file that is
/// compiled into this library. The binary knows its own contract's hash: HELLO carries it, and
/// [`accept`] compares against it. A malformed fingerprint file is a broken build rather than
/// hostile input, and panics as one.
#[must_use]
pub fn recorded_fingerprint() -> [u8; 32] {
    let text = include_str!("../include/lnk/protocol_fingerprint.txt");
    let hex = text
        .lines()
        .find_map(|line| line.strip_prefix("sha256="))
        .expect("protocol_fingerprint.txt carries a sha256= line; the build is broken otherwise");
    let mut fingerprint = [0u8; 32];
    for (index, byte) in fingerprint.iter_mut().enumerate() {
        let pair = hex.get(index * 2..index * 2 + 2).expect("the recorded sha256 is 64 hex characters");
        *byte = u8::from_str_radix(pair, 16).expect("the recorded sha256 is hexadecimal");
    }
    fingerprint
}

/// The HELLO this build sends: its own protocol version and its own header's fingerprint. The
/// caller chooses what it is — spectator or creature host — and which world it lives in, as
/// the fingerprint over its own [`crate::protocol::WorldDefinition`].
#[must_use]
pub fn local_hello(role: Role, world_fingerprint: u64) -> Hello {
    Hello {
        protocol_version: PROTOCOL_VERSION,
        fingerprint: recorded_fingerprint(),
        role: role as u8,
        reserved0: [0; 3],
        world_fingerprint,
    }
}

/// At most this many bytes of refusal text are read back after a closed handshake — enough for
/// any honest refusal, and a bound on a dishonest one.
pub const REFUSAL_LIMIT_BYTES: usize = 256;

/// Incremental frame reassembly with the header judged before the payload is read.
#[derive(Debug)]
struct FrameReader {
    buffer: Vec<u8>,
    filled: usize,
    /// Total frame size once the header has been judged; `None` while the header is short.
    expecting: Option<usize>,
}

impl FrameReader {
    fn new() -> Self {
        FrameReader {
            buffer: vec![0; max_frame_bytes()],
            filled: 0,
            expecting: None,
        }
    }

    /// Pump bytes from `source` towards one whole frame. `Ok(None)` means the socket had no
    /// more to give; call again later. `Ok(Some(..))` hands over a complete frame's type and
    /// payload and resets for the next.
    fn pump(&mut self, source: &mut impl Read) -> Result<Option<(u8, Vec<u8>)>, TransportError> {
        loop {
            let target = match self.expecting {
                Some(total) => total,
                None => FRAME_HEADER_BYTES,
            };

            while self.filled < target {
                match source.read(&mut self.buffer[self.filled..target]) {
                    Ok(0) => return Err(TransportError::PeerClosed),
                    Ok(read) => self.filled += read,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
                    Err(error) if error.kind() == ErrorKind::Interrupted => {}
                    Err(error) => return Err(error.into()),
                }
            }

            match self.expecting {
                None => {
                    let (length, type_byte) = decode_frame_header([self.buffer[0], self.buffer[1], self.buffer[2]]);
                    let rule = payload_rule(type_byte).map_err(TransportError::Frame)?;
                    check_length(rule, length as usize).map_err(TransportError::Frame)?;
                    self.expecting = Some(FRAME_HEADER_BYTES + length as usize);
                }
                Some(total) => {
                    let type_byte = self.buffer[2];
                    let payload = self.buffer[FRAME_HEADER_BYTES..total].to_vec();
                    self.filled = 0;
                    self.expecting = None;
                    return Ok(Some((type_byte, payload)));
                }
            }
        }
    }
}

/// The coalesced outgoing buffer: everything queued since the last flush leaves in as few
/// writes as the socket allows, and a partial write's remainder is carried, never dropped.
#[derive(Debug)]
struct WriteBuffer {
    pending: Vec<u8>,
    written: usize,
}

impl WriteBuffer {
    fn new() -> Self {
        WriteBuffer { pending: Vec::new(), written: 0 }
    }

    fn queue(&mut self, message: &Message) -> Result<(), EncodeError> {
        encode(message, &mut self.pending)
    }

    /// Push pending bytes into `sink`. `Ok(true)` means everything left; `Ok(false)` means the
    /// socket stopped accepting and the remainder is carried for the next flush.
    fn flush_into(&mut self, sink: &mut impl Write) -> Result<bool, TransportError> {
        if self.pending.len() > WRITE_BUFFER_LIMIT_BYTES {
            return Err(TransportError::WriteBufferOverflow);
        }
        while self.written < self.pending.len() {
            match sink.write(&self.pending[self.written..]) {
                Ok(0) => return Err(TransportError::PeerClosed),
                Ok(wrote) => self.written += wrote,
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(false),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        self.pending.clear();
        self.written = 0;
        Ok(true)
    }
}

/// A framed, non-blocking connection: the state machine both ends turn once per tick.
#[derive(Debug)]
pub struct Connection {
    stream: TcpStream,
    reader: FrameReader,
    writer: WriteBuffer,
    /// True on a server-held connection whose peer said spectator in HELLO: an ACTIONS frame
    /// arriving here is the role violation [`TransportError::ActionsFromSpectator`] names.
    forbid_incoming_actions: bool,
    /// True on a client-held connection that introduced itself as a spectator: the sending
    /// half's mirror of the same rule, enforced at the ABI's `send_actions`.
    forbid_outgoing_actions: bool,
}

impl Connection {
    fn framed(stream: TcpStream, forbid_incoming_actions: bool, forbid_outgoing_actions: bool) -> Result<Self, TransportError> {
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;
        stream.set_nonblocking(true)?;
        Ok(Connection {
            stream,
            reader: FrameReader::new(),
            writer: WriteBuffer::new(),
            forbid_incoming_actions,
            forbid_outgoing_actions,
        })
    }

    /// False exactly when this end introduced itself as a spectator: a spectator never sends
    /// ACTIONS or REZ, and the sending half refuses to stage either.
    #[must_use]
    pub fn may_send_intents(&self) -> bool {
        !self.forbid_outgoing_actions
    }

    /// One complete message if the socket holds one, `None` if it does not yet. Any refusal —
    /// unknown type, impossible length, a payload the codec rejects, ACTIONS from a spectator —
    /// is the connection's end: hang up and report, exactly as the doctrine demands.
    pub fn poll(&mut self) -> Result<Option<Message>, TransportError> {
        match self.reader.pump(&mut self.stream)? {
            None => Ok(None),
            Some((type_byte, payload)) => {
                let message = decode(type_byte, &payload).map_err(TransportError::Frame)?;
                if matches!(message, Message::Actions(_) | Message::Rez { .. }) && self.forbid_incoming_actions {
                    let _ = self.stream.shutdown(Shutdown::Both);
                    return Err(TransportError::ActionsFromSpectator);
                }
                Ok(Some(message))
            }
        }
    }

    /// Stage a message for the next flush. Refuses exactly what the codec refuses.
    pub fn queue(&mut self, message: &Message) -> Result<(), EncodeError> {
        self.writer.queue(message)
    }

    /// One coalesced write per tick: push everything queued. `Ok(false)` means the socket is
    /// full and the remainder is carried — call again next tick.
    pub fn flush(&mut self) -> Result<bool, TransportError> {
        self.writer.flush_into(&mut self.stream)
    }

    /// Say BYE and close. Best-effort: the peer may already be gone, and that is fine.
    pub fn close(mut self) {
        let _ = self.writer.queue(&Message::Bye);
        let _ = self.writer.flush_into(&mut self.stream);
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

fn normalise_timeout(error: std::io::Error) -> TransportError {
    match error.kind() {
        ErrorKind::WouldBlock | ErrorKind::TimedOut => TransportError::HandshakeTimedOut,
        _ => error.into(),
    }
}

fn read_exactly(stream: &mut TcpStream, bytes: &mut [u8]) -> Result<(), TransportError> {
    let mut filled = 0;
    while filled < bytes.len() {
        match stream.read(&mut bytes[filled..]) {
            Ok(0) => return Err(TransportError::PeerClosed),
            Ok(read) => filled += read,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(normalise_timeout(error)),
        }
    }
    Ok(())
}

/// Write a refusal line, shut the connection, and hand the same words back to the caller for
/// its own log. Best-effort on the wire: the refusal is a courtesy, the closure is the point.
fn refuse(stream: &TcpStream, reason: String) -> TransportError {
    let mut stream_ref = stream;
    let _ = stream_ref.write_all(reason.as_bytes());
    let _ = stream_ref.flush();
    let _ = stream.shutdown(Shutdown::Both);
    TransportError::Refused { reason }
}

/// The client half of the handshake: magic, HELLO, then either WELCOME or the server's refusal
/// line. Blocking, bounded by `timeout` per read. On success the connection is framed and
/// non-blocking, and the server's WELCOME is handed back beside it.
pub fn connect<A: ToSocketAddrs>(address: A, hello: &Hello, timeout: Duration) -> Result<(Connection, Welcome), TransportError> {
    let mut stream = TcpStream::connect(address)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let mut opening = Vec::with_capacity(MAGIC.len() + FRAME_HEADER_BYTES + size_of::<Hello>());
    opening.extend_from_slice(&MAGIC);
    encode(&Message::Hello(*hello), &mut opening).map_err(|_| TransportError::Garbled("a HELLO the codec itself refuses"))?;
    stream.write_all(&opening).map_err(normalise_timeout)?;

    let mut header = [0u8; FRAME_HEADER_BYTES];
    read_answer(&mut stream, &mut header)?;
    let (length, type_byte) = decode_frame_header(header);

    if type_byte != MessageType::Welcome as u8 || length as usize != size_of::<Welcome>() {
        // Not a WELCOME frame: by the handshake's contract this is the start of a refusal
        // line. Collect what the server managed to say before it closed.
        let mut reason = header.to_vec();
        let mut rest = [0u8; REFUSAL_LIMIT_BYTES];
        let mut collected = 0;
        while collected < rest.len() {
            match stream.read(&mut rest[collected..]) {
                Ok(0) => break,
                Ok(read) => collected += read,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        reason.extend_from_slice(&rest[..collected]);
        let reason = String::from_utf8_lossy(&reason).trim_end().to_string();
        return Err(TransportError::Refused { reason });
    }

    let mut payload = vec![0u8; length as usize];
    read_exactly(&mut stream, &mut payload)?;
    let Message::Welcome(welcome) = decode(type_byte, &payload).map_err(TransportError::Frame)? else {
        return Err(TransportError::Garbled("a WELCOME frame that decoded as something else"));
    };

    // The skew check's client half: a server living on a different floor is walked away from,
    // because a client must not perceive a world it would silently mis-place.
    if welcome.world_fingerprint != hello.world_fingerprint {
        let _ = stream.shutdown(Shutdown::Both);
        return Err(TransportError::Refused {
            reason: format!(
                "link: this end and the server live in different worlds - world fingerprint {:016X} here, {:016X} there - rebuild both ends from one world definition",
                hello.world_fingerprint, welcome.world_fingerprint
            ),
        });
    }

    Ok((Connection::framed(stream, false, hello.role == Role::Spectator as u8)?, welcome))
}

fn read_answer(stream: &mut TcpStream, header: &mut [u8; FRAME_HEADER_BYTES]) -> Result<(), TransportError> {
    match read_exactly(stream, header) {
        Err(TransportError::PeerClosed) => Err(TransportError::Refused {
            reason: "the server closed the connection without a word".to_string(),
        }),
        other => other,
    }
}

/// The server half of the handshake: expect the magic, expect a HELLO, compare the protocol
/// fingerprint against this build's own and the world fingerprint against the listener's, and
/// refuse in words — bad magic, wrong contract, invalid role, a different world — or hand back
/// the framed connection beside the client's HELLO. The caller sends WELCOME itself, promptly:
/// only it knows the current tick.
pub fn accept(stream: TcpStream, timeout: Duration, world_fingerprint: u64) -> Result<(Connection, Hello), TransportError> {
    let mut stream = stream;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let mut magic = [0u8; 4];
    read_exactly(&mut stream, &mut magic)?;
    if magic != MAGIC {
        return Err(refuse(&stream, "link: bad magic - this port speaks the Link protocol\n".to_string()));
    }

    let mut header = [0u8; FRAME_HEADER_BYTES];
    read_exactly(&mut stream, &mut header)?;
    let (length, type_byte) = decode_frame_header(header);
    if type_byte != MessageType::Hello as u8 || length as usize != size_of::<Hello>() {
        return Err(refuse(&stream, "link: the first frame must be HELLO\n".to_string()));
    }

    let mut payload = vec![0u8; length as usize];
    read_exactly(&mut stream, &mut payload)?;
    let hello = match decode(type_byte, &payload) {
        Ok(Message::Hello(hello)) => hello,
        Ok(_) => return Err(TransportError::Garbled("a HELLO frame that decoded as something else")),
        Err(DecodeError::InvalidRole(role)) => {
            return Err(refuse(&stream, format!("link: role {role} is neither spectator nor creature host\n")));
        }
        Err(error) => return Err(TransportError::Frame(error)),
    };

    if hello.fingerprint != recorded_fingerprint() {
        // Two builds can agree about the version number and still disagree about the bytes -
        // a modified header without a bump. Naming two equal numbers at each other would be
        // technically true and perfectly unhelpful, so that case gets its own words.
        let reason = if hello.protocol_version == PROTOCOL_VERSION {
            format!(
                "link: protocol fingerprint mismatch at the same version {}: one of us carries a modified lnk_protocol.h - rebuild both ends from the same header\n",
                PROTOCOL_VERSION
            )
        } else {
            format!(
                "link: protocol fingerprint mismatch: you speak protocol version {}, this end speaks version {} - rebuild against this end's lnk_protocol.h\n",
                hello.protocol_version, PROTOCOL_VERSION
            )
        };
        return Err(refuse(&stream, reason));
    }

    // The skew check's server half: a citizen of a different world is refused at the door,
    // because welcoming it would let two floors disagree about where every creature stands.
    if hello.world_fingerprint != world_fingerprint {
        return Err(refuse(
            &stream,
            format!(
                "link: you live in a different world - world fingerprint {:016X} yours, {:016X} this world's - rebuild both ends from one world definition\n",
                hello.world_fingerprint, world_fingerprint
            ),
        ));
    }

    Ok((Connection::framed(stream, hello.role == Role::Spectator as u8, false)?, hello))
}

/// The listening half: Master Control's socket, or a test playing Master Control's part.
/// Binds 127.0.0.1 only while the trust stance holds — a world reachable from elsewhere is the
/// trigger the deferred security tier waits behind — and accepts without blocking, so a tick
/// loop can ask "did anybody knock?" and move on.
#[derive(Debug)]
pub struct Listener {
    listener: TcpListener,
}

/// Bind the listening socket. Port 0 asks the operating system for any free port;
/// [`Listener::port`] answers which.
pub fn listen(port: u16) -> Result<Listener, TransportError> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    Ok(Listener { listener })
}

impl Listener {
    /// One pending connection if somebody knocked, `None` if nobody has. The stream is still
    /// raw — hand it to [`accept`] to walk the handshake.
    pub fn knock(&self) -> Result<Option<TcpStream>, TransportError> {
        match self.listener.accept() {
            Ok((stream, _)) => Ok(Some(stream)),
            Err(error) if error.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// The port actually bound — the answer to `listen(0)`, and the number a log should print.
    pub fn port(&self) -> Result<u16, TransportError> {
        Ok(self.listener.local_addr()?.port())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Actions, Rez, TICK_STATE_MAX_CREATURES};
    use std::net::TcpListener;

    const PATIENCE: Duration = Duration::from_secs(5);
    /// The one world every test here lives in - a fingerprint, not a definition, because the
    /// transport only ever compares the number.
    const WORLD: u64 = 0x5EED_0F7E_601D;

    fn loopback() -> (TcpListener, std::net::SocketAddr) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let address = listener.local_addr().expect("bound address");
        (listener, address)
    }

    fn welcome() -> Message {
        Message::Welcome(Welcome {
            current_tick: 100,
            nominal_dt_seconds: 0.03125,
            client_id: 1,
            world_fingerprint: WORLD,
        })
    }

    fn actions(tick: u64) -> Message {
        Message::Actions(Actions {
            tick,
            creature_id: 1,
            desired_forward_speed: 1.0,
            desired_turn_rate: 0.0,
            vocalisation_strength: 0.0,
            previous_forward_speed: 0.0,
            previous_turn_rate: 0.0,
            previous_vocalisation: 0.0,
            reserved0: [0; 4],
        })
    }

    fn poll_until_message(connection: &mut Connection) -> Message {
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            if let Some(message) = connection.poll().expect("polling a healthy connection") {
                return message;
            }
            assert!(std::time::Instant::now() < deadline, "no frame arrived within patience");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn the_handshake_succeeds_and_frames_flow_in_order() {
        let (listener, address) = loopback();

        let client = std::thread::spawn(move || {
            let (mut connection, welcome) = connect(address, &local_hello(Role::CreatureHost, WORLD), PATIENCE).expect("handshake");
            assert_eq!(welcome.current_tick, 100);
            connection.queue(&actions(101)).expect("queue");
            connection.queue(&actions(102)).expect("queue");
            assert!(connection.flush().expect("flush"), "loopback accepts a coalesced write whole");
        });

        let (stream, _) = listener.accept().expect("accept");
        let (mut connection, hello) = accept(stream, PATIENCE, WORLD).expect("server handshake");
        assert_eq!(hello.role, Role::CreatureHost as u8);
        assert_eq!(hello.protocol_version, PROTOCOL_VERSION);
        connection.queue(&welcome()).expect("queue WELCOME");
        assert!(connection.flush().expect("flush WELCOME"));

        assert_eq!(poll_until_message(&mut connection), actions(101), "first queued, first delivered");
        assert_eq!(poll_until_message(&mut connection), actions(102), "order is the transport's promise");
        client.join().expect("client thread");
    }

    #[test]
    fn bad_magic_is_refused_in_words_the_client_can_read() {
        let (listener, address) = loopback();

        let client = std::thread::spawn(move || {
            let mut raw = TcpStream::connect(address).expect("raw connect");
            raw.write_all(b"HTTP").expect("write wrong magic");
            let mut answer = Vec::new();
            raw.set_read_timeout(Some(PATIENCE)).expect("timeout");
            let _ = raw.read_to_end(&mut answer);
            String::from_utf8_lossy(&answer).to_string()
        });

        let (stream, _) = listener.accept().expect("accept");
        let refusal = accept(stream, PATIENCE, WORLD);
        let Err(TransportError::Refused { reason }) = refusal else {
            panic!("bad magic must be a worded refusal, got {refusal:?}");
        };
        assert!(reason.contains("bad magic"));

        let heard = client.join().expect("client thread");
        assert!(heard.contains("bad magic"), "the refusal must reach the client verbatim, got: {heard}");
    }

    #[test]
    fn a_wrong_fingerprint_is_refused_naming_both_versions() {
        let (listener, address) = loopback();

        let client = std::thread::spawn(move || {
            let mut wrong = local_hello(Role::Spectator, WORLD);
            wrong.fingerprint[0] ^= 0xFF;
            let error = connect(address, &wrong, PATIENCE).expect_err("a wrong fingerprint must not connect");
            let TransportError::Refused { reason } = error else {
                panic!("expected a worded refusal, got {error:?}");
            };
            reason
        });

        let (stream, _) = listener.accept().expect("accept");
        assert!(matches!(accept(stream, PATIENCE, WORLD), Err(TransportError::Refused { .. })));

        let reason = client.join().expect("client thread");
        assert!(reason.contains("fingerprint mismatch"), "got: {reason}");
        assert!(
            reason.contains(&format!("same version {PROTOCOL_VERSION}")) && reason.contains("modified"),
            "equal versions with differing bytes must be named as a modified header, not two equal numbers - got: {reason}"
        );
    }

    #[test]
    fn a_wrong_fingerprint_from_another_version_names_both_versions() {
        let (listener, address) = loopback();

        let client = std::thread::spawn(move || {
            let mut wrong = local_hello(Role::Spectator, WORLD);
            wrong.fingerprint[0] ^= 0xFF;
            wrong.protocol_version = PROTOCOL_VERSION + 1;
            let error = connect(address, &wrong, PATIENCE).expect_err("a wrong fingerprint must not connect");
            let TransportError::Refused { reason } = error else {
                panic!("expected a worded refusal, got {error:?}");
            };
            reason
        });

        let (stream, _) = listener.accept().expect("accept");
        assert!(matches!(accept(stream, PATIENCE, WORLD), Err(TransportError::Refused { .. })));

        let reason = client.join().expect("client thread");
        assert!(
            reason.contains(&format!("version {}", PROTOCOL_VERSION + 1)) && reason.contains(&format!("version {PROTOCOL_VERSION}")),
            "differing versions must both be named, got: {reason}"
        );
    }

    #[test]
    fn actions_from_a_spectator_end_the_connection_at_the_server() {
        let (listener, address) = loopback();

        // A hostile client: introduces itself as a spectator, then queues ACTIONS at the
        // transport layer, beneath the ABI's own sending-half refusal.
        let client = std::thread::spawn(move || {
            let (mut connection, _) = connect(address, &local_hello(Role::Spectator, WORLD), PATIENCE).expect("handshake");
            connection.queue(&actions(9)).expect("the codec itself does not know roles");
            // The server may hang up on the violation while this end is still flushing; that
            // is the refusal working, not a test failure.
            loop {
                match connection.flush() {
                    Ok(true) | Err(_) => break,
                    Ok(false) => {}
                }
            }
            connection
        });

        let (stream, _) = listener.accept().expect("accept");
        let (mut server_side, hello) = accept(stream, PATIENCE, WORLD).expect("handshake");
        assert_eq!(hello.role, Role::Spectator as u8);

        // The client's connect blocks on WELCOME; the violation only flows once it is a citizen.
        server_side
            .queue(&Message::Welcome(Welcome {
                current_tick: 1,
                nominal_dt_seconds: 0.031_25,
                client_id: 1,
                world_fingerprint: WORLD,
            }))
            .expect("queue welcome");
        while !server_side.flush().expect("flush welcome") {}

        let deadline = std::time::Instant::now() + PATIENCE;
        let verdict = loop {
            match server_side.poll() {
                Ok(None) => {
                    assert!(std::time::Instant::now() < deadline, "the violation never arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                other => break other,
            }
        };
        assert!(
            matches!(verdict, Err(TransportError::ActionsFromSpectator)),
            "a spectator's ACTIONS must end the connection, got {verdict:?}"
        );
        drop(client.join().expect("client thread"));
    }

    /// A bodiless REZ: every count zero, the one shape a spectator could plausibly try.
    fn bodiless_rez() -> Message {
        Message::Rez {
            header: Rez {
                creature_id: 1,
                max_forward_speed: 1.0,
                max_turn_rate: 1.0,
                max_vocalisation_strength: 1.0,
                max_contact_count: 4,
                vertex_count: 0,
                triangle_count: 0,
                material_count: 0,
            },
            vertices: Vec::new(),
            triangles: Vec::new(),
            materials: Vec::new(),
        }
    }

    #[test]
    fn rez_from_a_spectator_ends_the_connection_at_the_server_too() {
        let (listener, address) = loopback();
        let client = std::thread::spawn(move || {
            let (mut connection, _) = connect(address, &local_hello(Role::Spectator, WORLD), PATIENCE).expect("handshake");
            connection.queue(&bodiless_rez()).expect("the codec itself does not know roles");
            loop {
                match connection.flush() {
                    Ok(true) | Err(_) => break,
                    Ok(false) => {}
                }
            }
            connection
        });

        let (stream, _) = listener.accept().expect("accept");
        let (mut server_side, _) = accept(stream, PATIENCE, WORLD).expect("handshake");
        server_side.queue(&welcome()).expect("queue welcome");
        while !server_side.flush().expect("flush welcome") {}

        let deadline = std::time::Instant::now() + PATIENCE;
        let verdict = loop {
            match server_side.poll() {
                Ok(None) => {
                    assert!(std::time::Instant::now() < deadline, "the violation never arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                other => break other,
            }
        };
        assert!(
            matches!(verdict, Err(TransportError::ActionsFromSpectator)),
            "a spectator's REZ must end the connection like its ACTIONS would, got {verdict:?}"
        );
        drop(client.join().expect("client thread"));
    }

    #[test]
    fn a_different_world_is_refused_at_the_door_naming_both_fingerprints() {
        let (listener, address) = loopback();
        let client = std::thread::spawn(move || {
            let Err(TransportError::Refused { reason }) = connect(address, &local_hello(Role::CreatureHost, WORLD + 1), PATIENCE) else {
                panic!("a citizen of another world must be refused in words");
            };
            reason
        });

        let (stream, _) = listener.accept().expect("accept");
        assert!(matches!(accept(stream, PATIENCE, WORLD), Err(TransportError::Refused { .. })));

        let reason = client.join().expect("client thread");
        assert!(
            reason.contains(&format!("{:016X}", WORLD + 1)) && reason.contains(&format!("{WORLD:016X}")),
            "both worlds must be named, got: {reason}"
        );
    }

    #[test]
    fn a_server_from_a_different_world_is_refused_by_the_client() {
        // The server half of the check is skipped on purpose: a server that forgets to compare
        // (or lies in its WELCOME) is still caught, because the client compares too.
        let (listener, address) = loopback();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let (mut server_side, _) = accept(stream, PATIENCE, WORLD + 1).expect("this server accepts the other world");
            let Message::Welcome(mut welcome) = welcome() else { unreachable!() };
            welcome.world_fingerprint = WORLD; // the lie: it welcomed WORLD + 1
            server_side.queue(&Message::Welcome(welcome)).expect("queue welcome");
            let _ = server_side.flush();
        });

        let verdict = connect(address, &local_hello(Role::CreatureHost, WORLD + 1), PATIENCE);
        let Err(TransportError::Refused { reason }) = verdict else {
            panic!("a WELCOME from another world must be refused in words, got {verdict:?}");
        };
        assert!(reason.contains("different worlds"), "got: {reason}");
        server.join().expect("server thread");
    }

    #[test]
    fn an_unread_peer_cannot_grow_the_write_buffer_without_bound() {
        let mut writer = WriteBuffer {
            pending: vec![0u8; WRITE_BUFFER_LIMIT_BYTES + 1],
            written: 0,
        };
        let mut sink = Vec::new();
        assert!(
            matches!(writer.flush_into(&mut sink), Err(TransportError::WriteBufferOverflow)),
            "a megabyte of unread pending bytes is a dead peer, not a bigger buffer"
        );
        assert!(sink.is_empty(), "an overflowing buffer must not keep writing");
    }

    #[test]
    fn a_frame_dribbled_one_byte_at_a_time_still_arrives_whole() {
        let (listener, address) = loopback();

        let client = std::thread::spawn(move || {
            // A creature host, because the dribbled frame is ACTIONS and a spectator's would
            // now be refused as the role violation it is.
            let (connection, _) = connect(address, &local_hello(Role::CreatureHost, WORLD), PATIENCE).expect("handshake");
            let mut frame = Vec::new();
            encode(&actions(7), &mut frame).expect("encode");
            let mut raw = connection.stream;
            raw.set_nonblocking(false).expect("blocking for the dribble");
            for byte in frame {
                raw.write_all(&[byte]).expect("dribble");
                raw.flush().expect("flush the single byte");
                std::thread::sleep(Duration::from_millis(1));
            }
        });

        let (stream, _) = listener.accept().expect("accept");
        let (mut connection, _) = accept(stream, PATIENCE, WORLD).expect("server handshake");
        connection.queue(&welcome()).expect("queue");
        assert!(connection.flush().expect("flush"));

        assert_eq!(poll_until_message(&mut connection), actions(7), "reassembly across arbitrarily small reads");
        client.join().expect("client thread");
    }

    #[test]
    fn a_hostile_length_hangs_up_at_the_header() {
        let (listener, address) = loopback();

        let client = std::thread::spawn(move || {
            let (connection, _) = connect(address, &local_hello(Role::Spectator, WORLD), PATIENCE).expect("handshake");
            let mut raw = connection.stream;
            raw.set_nonblocking(false).expect("blocking");
            // Length 65535 declared for a HELLO, whose rule is exactly 48: refused from the
            // header alone, no payload ever read.
            raw.write_all(&[0xFF, 0xFF, MessageType::Hello as u8]).expect("hostile header");
            raw.flush().expect("flush");
        });

        let (stream, _) = listener.accept().expect("accept");
        let (mut connection, _) = accept(stream, PATIENCE, WORLD).expect("server handshake");
        connection.queue(&welcome()).expect("queue");
        assert!(connection.flush().expect("flush"));

        let deadline = std::time::Instant::now() + PATIENCE;
        let verdict = loop {
            match connection.poll() {
                Ok(None) => {
                    assert!(std::time::Instant::now() < deadline, "the hostile header was never judged");
                    std::thread::sleep(Duration::from_millis(1));
                }
                other => break other,
            }
        };
        assert!(
            matches!(verdict, Err(TransportError::Frame(DecodeError::WrongLength { expected: 48, got: 65535 }))),
            "got {verdict:?}"
        );
        client.join().expect("client thread");
    }

    /// A sink that accepts five bytes per call: the partial-write carry, forced determinist­ically.
    struct Dribbling {
        accepted: Vec<u8>,
    }

    impl Write for Dribbling {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let take = bytes.len().min(5);
            self.accepted.extend_from_slice(&bytes[..take]);
            Ok(take)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_partial_write_carries_its_remainder_rather_than_dropping_it() {
        let mut writer = WriteBuffer::new();
        let big = Message::TickState {
            header: crate::protocol::TickStateHeader {
                tick: 9,
                creature_count: TICK_STATE_MAX_CREATURES,
                reserved0: [0; 4],
            },
            states: vec![
                crate::protocol::CreatureState {
                    creature_id: 1,
                    position: [0.0, 1.0, 2.0],
                    yaw: 0.1,
                    velocity: [0.0, 0.0, 0.0],
                    yaw_rate: 0.0,
                    vocalisation: 0.0,
                };
                TICK_STATE_MAX_CREATURES as usize
            ],
        };
        writer.queue(&big).expect("queue");

        let mut expected = Vec::new();
        encode(&big, &mut expected).expect("encode");

        let mut sink = Dribbling { accepted: Vec::new() };
        while !writer.flush_into(&mut sink).expect("flushing into a slow sink") {}
        assert_eq!(sink.accepted, expected, "every byte, once each, in order");
    }

    #[test]
    fn the_recorded_fingerprint_parses_and_is_not_nothing() {
        let fingerprint = recorded_fingerprint();
        assert_ne!(fingerprint, [0u8; 32]);
        assert_eq!(local_hello(Role::Spectator, WORLD).fingerprint, fingerprint);
    }
}
