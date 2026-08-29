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
    /// A message arrived at the end that only sends it: WELCOME, TICK_STATE, EVENT or
    /// PROPRIOCEPTION at the server, HELLO or ACTIONS at the client. Every message flows one
    /// way, and an end that speaks the other's words is violating the protocol - an interval
    /// refuses both directions of nonsense. The name says which; the connection is shut first.
    WrongWay(&'static str),
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

/// How long [`Connection::close`] waits for the peer to close its half after BYE, discarding
/// whatever it still sends: a bound on a peer that never hangs up, generous against a healthy
/// one that answers within a tick or two.
pub const FAREWELL_WINDOW: Duration = Duration::from_millis(250);

/// At most this many bytes of refusal text are read back after a closed handshake — enough for
/// any honest refusal, and a bound on a dishonest one.
pub const REFUSAL_LIMIT_BYTES: usize = 256;

/// Incremental frame reassembly with the header judged before the payload is read.
#[derive(Debug)]
pub(crate) struct FrameReader {
    buffer: Vec<u8>,
    filled: usize,
    /// Total frame size once the header has been judged; `None` while the header is short.
    expecting: Option<usize>,
}

impl FrameReader {
    pub(crate) fn new() -> Self {
        FrameReader {
            buffer: vec![0; max_frame_bytes()],
            filled: 0,
            expecting: None,
        }
    }

    /// Pump bytes from `source` towards one whole frame. `Ok(None)` means the socket had no
    /// more to give; call again later. `Ok(Some(..))` hands over a complete frame's type and
    /// payload and resets for the next.
    pub(crate) fn pump(&mut self, source: &mut impl Read) -> Result<Option<(u8, Vec<u8>)>, TransportError> {
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
pub(crate) struct WriteBuffer {
    pending: Vec<u8>,
    written: usize,
}

impl WriteBuffer {
    pub(crate) fn new() -> Self {
        WriteBuffer { pending: Vec::new(), written: 0 }
    }

    pub(crate) fn queue(&mut self, message: &Message) -> Result<(), EncodeError> {
        encode(message, &mut self.pending)
    }

    /// Bytes staged and not yet written.
    pub(crate) fn pending_bytes(&self) -> usize {
        self.pending.len() - self.written
    }

    /// Push pending bytes into `sink`. `Ok(true)` means everything left; `Ok(false)` means the
    /// socket stopped accepting and the remainder is carried for the next flush.
    pub(crate) fn flush_into(&mut self, sink: &mut impl Write) -> Result<bool, TransportError> {
        while self.written < self.pending.len() {
            match sink.write(&self.pending[self.written..]) {
                Ok(0) => return Err(TransportError::PeerClosed),
                Ok(wrote) => self.written += wrote,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    // The limit is on what the peer left unread, judged after the socket took
                    // what it would: a big batch to a reading peer is fine, a megabyte a peer
                    // will not read is the end.
                    if self.pending_bytes() > WRITE_BUFFER_LIMIT_BYTES {
                        return Err(TransportError::WriteBufferOverflow);
                    }
                    return Ok(false);
                }
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
    /// True on a server-held connection: the one end that may send PROPRIOCEPTION, and the one
    /// end that must never receive it.
    server_held: bool,
}

impl Connection {
    fn framed(stream: TcpStream, forbid_incoming_actions: bool, forbid_outgoing_actions: bool, server_held: bool) -> Result<Self, TransportError> {
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;
        stream.set_nonblocking(true)?;
        Ok(Connection {
            stream,
            reader: FrameReader::new(),
            writer: WriteBuffer::new(),
            forbid_incoming_actions,
            forbid_outgoing_actions,
            server_held,
        })
    }

    /// Whether this end may send PROPRIOCEPTION: only a server-held connection.
    #[must_use]
    pub fn may_send_proprioception(&self) -> bool {
        self.server_held
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
                let message = match decode(type_byte, &payload) {
                    Ok(message) => message,
                    Err(refusal) => {
                        // The connection is over, as the header says - shut now, so a later
                        // close does not bid a hostile peer a polite farewell.
                        let _ = self.stream.shutdown(Shutdown::Both);
                        return Err(TransportError::Frame(refusal));
                    }
                };
                if matches!(message, Message::Actions(_) | Message::Rez { .. }) && self.forbid_incoming_actions {
                    let _ = self.stream.shutdown(Shutdown::Both);
                    return Err(TransportError::ActionsFromSpectator);
                }
                let wrong_way = if self.server_held {
                    match message {
                        Message::Welcome(_) => Some("WELCOME"),
                        Message::TickState { .. } => Some("TICK_STATE"),
                        Message::Event(_) => Some("EVENT"),
                        Message::Proprioception { .. } => Some("PROPRIOCEPTION"),
                        Message::Refused(_) => Some("REFUSED"),
                        _ => None,
                    }
                } else {
                    match message {
                        Message::Hello(_) => Some("HELLO"),
                        Message::Actions(_) => Some("ACTIONS"),
                        _ => None,
                    }
                };
                if let Some(name) = wrong_way {
                    let _ = self.stream.shutdown(Shutdown::Both);
                    return Err(TransportError::WrongWay(name));
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
        // The farewell is owed the whole window, not one non-blocking try: a send buffer full of
        // the last tick would otherwise cut the carried remainder - half a frame and the BYE -
        // and the peer would read a leave as a crash.
        let _ = self.stream.set_nonblocking(false);
        let _ = self.stream.set_write_timeout(Some(FAREWELL_WINDOW));
        let _ = self.writer.flush_into(&mut self.stream);
        // An orderly release, not a slam: close only the writing half, then read and discard
        // until the peer closes too (or the farewell window lapses). Slamming both halves while
        // the peer still has a tick in flight makes this end answer that tick with a TCP reset,
        // and a reset discards the peer's unread receive buffer - BYE included - so a polite
        // leave would arrive there as a crash. Found the honest way: the first per-owner
        // letter made the server write often enough to lose the farewell.
        let _ = self.stream.shutdown(Shutdown::Write);
        let _ = self.stream.set_nonblocking(false);
        let _ = self.stream.set_read_timeout(Some(FAREWELL_WINDOW));
        let deadline = std::time::Instant::now() + FAREWELL_WINDOW;
        let mut sink = [0u8; 1024];
        while std::time::Instant::now() < deadline {
            match self.stream.read(&mut sink) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
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
    // A wrong client - an HTTP request, a TLS hello - has usually said more than was read, and
    // closing on unread bytes sends a reset that discards the refusal at the peer. Drain what
    // is already here, without waiting for more, so the words can land.
    if stream.set_nonblocking(true).is_ok() {
        let mut sink = [0u8; 4096];
        for _ in 0..16 {
            match stream_ref.read(&mut sink) {
                Ok(read) if read > 0 => {}
                _ => break,
            }
        }
        let _ = stream.set_nonblocking(false);
    }
    let _ = stream_ref.write_all(reason.as_bytes());
    let _ = stream_ref.flush();
    let _ = stream.shutdown(Shutdown::Both);
    TransportError::Refused { reason }
}

/// The client half of the handshake: magic, HELLO, then either WELCOME or the server's refusal
/// line. Blocking, bounded by `timeout` - the TCP connect itself included, per address the name
/// resolves to, and then per read. On success the connection is framed and non-blocking, and
/// the server's WELCOME is handed back beside it. A zero timeout is refused: it would wait for
/// nothing and call that a timeout.
pub fn connect<A: ToSocketAddrs>(address: A, hello: &Hello, timeout: Duration) -> Result<(Connection, Welcome), TransportError> {
    if timeout.is_zero() {
        return Err(TransportError::Garbled("a zero timeout waits for nothing - give the handshake a bound"));
    }
    let mut stream = connect_bounded(address, timeout)?;
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
        let patience = std::time::Instant::now() + timeout;
        while collected < rest.len() && std::time::Instant::now() < patience {
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

    Ok((Connection::framed(stream, false, hello.role == Role::Spectator as u8, false)?, welcome))
}

/// `TcpStream::connect` with the timeout applied to the connect itself, every resolved address
/// tried in turn; a black-holed host costs the timeout, not the operating system's minutes.
fn connect_bounded<A: ToSocketAddrs>(address: A, timeout: Duration) -> Result<TcpStream, TransportError> {
    let deadline = std::time::Instant::now() + timeout;
    let candidates: Vec<_> = address.to_socket_addrs()?.collect();
    let mut last = None;
    loop {
        for candidate in &candidates {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                break;
            }
            match TcpStream::connect_timeout(candidate, left) {
                Ok(stream) => return Ok(stream),
                Err(error) => last = Some(error),
            }
        }
        // A refused or reset dial is retried within the budget: a server still standing up, a
        // listener mid-accept. The timeout bounds the whole, and the last refusal is the answer.
        let transient = matches!(
            last.as_ref().map(std::io::Error::kind),
            Some(ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset | ErrorKind::WouldBlock)
        );
        if !transient || std::time::Instant::now() + Duration::from_millis(10) >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(match last {
        Some(error) if error.kind() == ErrorKind::TimedOut || error.kind() == ErrorKind::WouldBlock => TransportError::HandshakeTimedOut,
        Some(error) => TransportError::Io(error.kind()),
        None => TransportError::Io(ErrorKind::InvalidInput),
    })
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
    if timeout.is_zero() {
        return Err(TransportError::Garbled("a zero timeout waits for nothing - give the handshake a bound"));
    }
    let mut stream = stream;
    // The listener is non-blocking and, on Windows, an accepted socket inherits that: a read
    // before the client's bytes land would answer WouldBlock, and the handshake would report
    // a timeout that never elapsed. The handshake blocks, bounded by the timeout, as promised.
    stream.set_nonblocking(false)?;
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

    Ok((Connection::framed(stream, hello.role == Role::Spectator as u8, false, true)?, hello))
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
    use crate::protocol::{Actions, Proprioception, Rez, TICK_STATE_MAX_CREATURES};
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
            joint_targets: [0.0; 7],
            previous_joint_targets: [0.0; 7],
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
                segment_count: 1,
                segment_spacing: 0.0,
                max_joint_angle: 0.0,
                max_joint_torque: 0.0,
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
    fn a_refusal_flows_one_way_only() {
        // The world's word on a REZ is the world's to say: a client that says it is hung up on.
        let (listener, address) = loopback();
        let refusal = Message::Refused(crate::protocol::Refused {
            tick: 1,
            creature_id: 1,
            reason: crate::protocol::RefusalReason::Owned as u8,
            reserved0: [0; 3],
        });
        let client_refusal = refusal.clone();
        let client = std::thread::spawn(move || {
            let (mut connection, _) = connect(address, &local_hello(Role::CreatureHost, WORLD), PATIENCE).expect("handshake");
            connection.queue(&client_refusal).expect("the codec itself does not know ends");
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
        server_side.queue(&refusal).expect("the server may refuse");
        while !server_side.flush().expect("flush") {}

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
            matches!(verdict, Err(TransportError::WrongWay("REFUSED"))),
            "a refusal arriving at the server ends the connection, got {verdict:?}"
        );
        drop(client.join().expect("client thread"));
    }

    #[test]
    fn proprioception_flows_one_way_only() {
        let (listener, address) = loopback();
        let letter = Message::Proprioception {
            header: Proprioception {
                tick: 1,
                creature_id: 1,
                grounded: 1,
                reserved0: [0; 3],
                specific_force: [0.0, 9.81, 0.0],
                contact_count: 0,
            },
            contacts: Vec::new(),
        };
        let client_letter = letter.clone();
        let client = std::thread::spawn(move || {
            let (mut connection, _) = connect(address, &local_hello(Role::CreatureHost, WORLD), PATIENCE).expect("handshake");
            assert!(!connection.may_send_proprioception(), "a client-held connection never sends the letter");
            // Beneath the ABI's refusal, the transport still carries it - and the server hangs up.
            connection.queue(&client_letter).expect("the codec itself does not know ends");
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
        assert!(server_side.may_send_proprioception(), "the server-held end is the one that may");
        server_side.queue(&welcome()).expect("queue welcome");
        server_side.queue(&letter).expect("queue the letter");
        while !server_side.flush().expect("flush") {}

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
            matches!(verdict, Err(TransportError::WrongWay("PROPRIOCEPTION"))),
            "a letter arriving at the server ends the connection, got {verdict:?}"
        );
        drop(client.join().expect("client thread"));
    }

    #[test]
    fn every_message_flows_its_own_way_only() {
        // The server's words at the server, the client's words at the client: each one ends
        // the connection, because an interval refuses both directions of nonsense. The
        // letter's own test above is the first of these; here are the rest.
        let tick_state = Message::TickState {
            header: crate::protocol::TickStateHeader {
                tick: 1,
                creature_count: 0,
                reserved0: [0; 4],
            },
            states: Vec::new(),
        };
        let event = Message::Event(crate::protocol::Event {
            tick: 1,
            position: [0.0; 3],
            strength: 1.0,
            creature_id: 1,
            kind: crate::protocol::EventKind::Vocalisation as u8,
            reserved0: [0; 3],
        });
        let actions = Message::Actions(Actions {
            tick: 2,
            creature_id: 1,
            desired_forward_speed: 1.0,
            desired_turn_rate: 0.0,
            vocalisation_strength: 0.0,
            previous_forward_speed: 0.0,
            previous_turn_rate: 0.0,
            previous_vocalisation: 0.0,
            joint_targets: [0.0; 7],
            previous_joint_targets: [0.0; 7],
            reserved0: [0; 4],
        });
        let hello = Message::Hello(local_hello(Role::Spectator, WORLD));

        // At the server: what only a server says.
        for (name, nonsense) in [("WELCOME", welcome()), ("TICK_STATE", tick_state.clone()), ("EVENT", event.clone())] {
            let (listener, address) = loopback();
            let client = std::thread::spawn(move || {
                let (mut connection, _) = connect(address, &local_hello(Role::CreatureHost, WORLD), PATIENCE).expect("handshake");
                connection.queue(&nonsense).expect("the codec itself does not know ends");
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
            while !server_side.flush().expect("flush") {}
            let verdict = await_verdict(&mut server_side);
            assert!(
                matches!(verdict, Err(TransportError::WrongWay(_))),
                "{name} arriving at the server ends the connection, got {verdict:?}"
            );
            drop(client.join().expect("client thread"));
        }

        // At the client: what only a client says.
        for (name, nonsense) in [("ACTIONS", actions), ("HELLO", hello)] {
            let (listener, address) = loopback();
            let server = std::thread::spawn(move || {
                let (stream, _) = listener.accept().expect("accept");
                let (mut server_side, _) = accept(stream, PATIENCE, WORLD).expect("handshake");
                server_side.queue(&welcome()).expect("queue welcome");
                server_side.queue(&nonsense).expect("the codec itself does not know ends");
                while !server_side.flush().expect("flush") {}
                server_side
            });
            let (mut connection, _) = connect(address, &local_hello(Role::CreatureHost, WORLD), PATIENCE).expect("handshake");
            let verdict = await_verdict(&mut connection);
            assert!(
                matches!(verdict, Err(TransportError::WrongWay(_))),
                "{name} arriving at the client ends the connection, got {verdict:?}"
            );
            drop(server.join().expect("server thread"));
        }
    }

    /// Poll until something other than "nothing yet", within patience.
    fn await_verdict(connection: &mut Connection) -> Result<Option<Message>, TransportError> {
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            match connection.poll() {
                Ok(None) => {
                    assert!(std::time::Instant::now() < deadline, "the verdict never arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                other => break other,
            }
        }
    }

    #[test]
    fn a_bye_survives_a_peer_that_is_still_writing() {
        // The server is mid-conversation: it writes a whole tick after the client has already
        // said BYE and closed. A slammed socket would answer that write with a reset and the
        // BYE would be lost with the reset; an orderly release lets the farewell land.
        let (listener, address) = loopback();
        let client = std::thread::spawn(move || {
            let (connection, _) = connect(address, &local_hello(Role::Spectator, WORLD), PATIENCE).expect("handshake");
            connection.close();
        });
        let (stream, _) = listener.accept().expect("accept");
        let (mut server_side, _) = accept(stream, PATIENCE, WORLD).expect("handshake");
        server_side.queue(&welcome()).expect("queue welcome");
        while !server_side.flush().expect("flush welcome") {}
        // Let the client reach its farewell: writing half closed, draining for FAREWELL_WINDOW.
        std::thread::sleep(FAREWELL_WINDOW / 5);

        // The peer has closed its writing half and is draining; write a fat tick at it anyway.
        let rows = (0..TICK_STATE_MAX_CREATURES)
            .map(|index| crate::protocol::CreatureState {
                creature_id: index,
                position: [0.0; 3],
                yaw: 0.0,
                velocity: [0.0; 3],
                yaw_rate: 0.0,
                vocalisation: 0.0,
                segment_count: 1,
                segments: [crate::protocol::SegmentPose::default(); crate::protocol::TRAILING_SEGMENTS_MAX],
            })
            .collect();
        let _ = server_side.queue(&Message::TickState {
            header: crate::protocol::TickStateHeader {
                tick: 1,
                creature_count: TICK_STATE_MAX_CREATURES,
                reserved0: [0; 4],
            },
            states: rows,
        });
        let _ = server_side.flush();

        let deadline = std::time::Instant::now() + PATIENCE;
        let verdict = loop {
            match server_side.poll() {
                Ok(None) => {
                    assert!(std::time::Instant::now() < deadline, "nothing arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                other => break other,
            }
        };
        assert!(
            matches!(verdict, Ok(Some(Message::Bye))),
            "the farewell must survive the peer's last tick, got {verdict:?}"
        );
        client.join().expect("client thread");
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

    /// A sink that takes nothing: the socket of a peer that has stopped reading.
    struct Deaf;

    impl Write for Deaf {
        fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(ErrorKind::WouldBlock))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn an_unread_peer_cannot_grow_the_write_buffer_without_bound_but_a_reading_one_may_take_a_big_batch() {
        // A peer that left more than a megabyte unread is dead.
        let mut writer = WriteBuffer {
            pending: vec![0u8; WRITE_BUFFER_LIMIT_BYTES + 1],
            written: 0,
        };
        assert!(
            matches!(writer.flush_into(&mut Deaf), Err(TransportError::WriteBufferOverflow)),
            "a megabyte of unread pending bytes is a dead peer, not a bigger buffer"
        );
        // The same batch to a peer that reads it is just a big tick - a late joiner told every
        // body at once - and leaves whole.
        let mut writer = WriteBuffer {
            pending: vec![0u8; 2 * WRITE_BUFFER_LIMIT_BYTES],
            written: 0,
        };
        let mut sink = Vec::new();
        assert!(matches!(writer.flush_into(&mut sink), Ok(true)), "a reading peer takes the batch");
        assert_eq!(sink.len(), 2 * WRITE_BUFFER_LIMIT_BYTES);
        // And a little left unread is carried, not condemned.
        let mut writer = WriteBuffer {
            pending: vec![0u8; 100],
            written: 0,
        };
        assert!(matches!(writer.flush_into(&mut Deaf), Ok(false)));
        assert_eq!(writer.pending_bytes(), 100);
    }

    #[test]
    fn a_client_that_hesitates_after_dialling_is_waited_for_not_timed_out_at_once() {
        // The listener is non-blocking, and an accepted socket inherits that on Windows: the
        // handshake must block for the client's bytes, bounded by the timeout, rather than
        // read WouldBlock and call it a timeout that never elapsed. The flake this pins came
        // once in a dozen runs; the hesitation makes it every time.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("non-blocking, as the Listener is");
        let address = listener.local_addr().expect("address");
        let client = std::thread::spawn(move || {
            let mut raw = TcpStream::connect(address).expect("dial");
            std::thread::sleep(Duration::from_millis(200));
            let mut opening = Vec::new();
            opening.extend_from_slice(&MAGIC);
            encode(&Message::Hello(local_hello(Role::Spectator, WORLD)), &mut opening).expect("encode");
            raw.write_all(&opening).expect("hello, late");
            raw.flush().expect("flush");
            let _ = raw.read(&mut [0u8; 64]); // until the server hangs up
        });
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(1)),
                Err(error) => panic!("{error}"),
            }
        };
        let verdict = accept(stream, PATIENCE, WORLD);
        assert!(verdict.is_ok(), "a late HELLO within the timeout is a handshake, got {:?}", verdict.err());
        drop(verdict);
        client.join().expect("client");
    }

    #[test]
    fn a_zero_timeout_is_refused_and_a_black_hole_costs_the_timeout_not_minutes() {
        let (listener, address) = loopback();
        assert!(
            matches!(
                connect(address, &local_hello(Role::Spectator, WORLD), Duration::ZERO),
                Err(TransportError::Garbled(_))
            ),
            "a zero timeout waits for nothing"
        );
        let (stream, _) = {
            let client = std::net::TcpStream::connect(address).expect("dial");
            (listener.accept().expect("accept").0, client)
        };
        assert!(matches!(accept(stream, Duration::ZERO, WORLD), Err(TransportError::Garbled(_))));

        // TEST-NET-1 (RFC 5737) is never routed: a connect there is a black hole on any honest
        // network, and with the bound it costs the timeout - not the operating system's minutes.
        let started = std::time::Instant::now();
        let verdict = connect("192.0.2.1:30702", &local_hello(Role::Spectator, WORLD), Duration::from_millis(300));
        assert!(verdict.is_err(), "nobody answers at a black hole");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the connect itself is bounded, took {:?}",
            started.elapsed()
        );
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
        // The connection is over, as the header says: the socket is shut, not merely judged.
        let mut probe = [0u8; 1];
        let _ = connection.stream.set_nonblocking(false);
        let _ = connection.stream.set_read_timeout(Some(Duration::from_millis(500)));
        assert!(!matches!(connection.stream.read(&mut probe), Ok(1)), "a refused connection must be shut");
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
                    segment_count: 1,
                    segments: [crate::protocol::SegmentPose::default(); crate::protocol::TRAILING_SEGMENTS_MAX],
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
