//! The C boundary: the one module where `unsafe` is allowed, because the unsafety is its job.
//!
//! Everything here mirrors `include/lnk/lnk_client.h`, which is the surface a consumer that
//! `LoadLibrary()`s or `dlopen()`s Link actually sees: one exported symbol returning a vtable,
//! NULL when the asked-for version cannot be satisfied — the flagship's `tglGetProgramVTable`
//! refusal, reproduced because it has already proven itself. Every function behind the table
//! catches unwinding at the edge and answers with a status code: no panic crosses this
//! boundary, ever, which is the noexcept doctrine in library clothing. Null pointers are
//! refused with `LNK_BAD_ARGUMENT` rather than dereferenced with hope.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use crate::codec::Message;
use crate::protocol::{Actions, CreatureState, Derez, Event, Hello, MessageType, PROTOCOL_VERSION, Ping, Pong, Role, TickStateHeader, Welcome};
use crate::transport::{Connection, TransportError, connect, local_hello, recorded_fingerprint};

/// `LNK_CLIENT_ABI_VERSION`: bumped whenever the vtable or its rules change. The twin lives in
/// `lnk_client.h`, and a test holds the two together.
pub const LNK_CLIENT_ABI_VERSION: u32 = 1;

pub type LnkStatus = i32;

pub const LNK_OK: LnkStatus = 0;
pub const LNK_NOTHING_YET: LnkStatus = 1;
pub const LNK_REFUSED: LnkStatus = 2;
pub const LNK_HANDSHAKE_TIMED_OUT: LnkStatus = 3;
pub const LNK_PEER_CLOSED: LnkStatus = 4;
pub const LNK_FRAME_REFUSED: LnkStatus = 5;
pub const LNK_GARBLED: LnkStatus = 6;
pub const LNK_IO: LnkStatus = 7;
pub const LNK_BAD_ARGUMENT: LnkStatus = 8;
pub const LNK_PANIC: LnkStatus = 9;

/// The opaque handle the C side holds. Never constructed as itself: it is the public face of a
/// pointer to [`ClientInner`], and only the casts below relate the two.
#[repr(C)]
pub struct LnkClient {
    _opaque: [u8; 0],
}

struct ClientInner {
    connection: Connection,
    /// The rows the last TICK_STATE view borrows from. Replaced on the next TICK_STATE, freed
    /// on close — exactly the lifetime the header promises the C side.
    tick_rows: Vec<CreatureState>,
}

/// `LnkTickStateView`: the header by value, the rows by borrow from [`ClientInner::tick_rows`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TickStateView {
    pub header: TickStateHeader,
    pub states: *const CreatureState,
}

/// The union behind `LnkMessageView.as`. Every member is plain old data; reading the member the
/// `type` byte names is the C side's contract, mirrored in the tests here.
#[repr(C)]
#[derive(Clone, Copy)]
pub union MessageViewPayload {
    pub welcome: Welcome,
    pub tick_state: TickStateView,
    pub event: Event,
    pub derez: Derez,
    pub ping: Ping,
    pub pong: Pong,
    pub hello: Hello,
    pub actions: Actions,
}

/// `LnkMessageView`, field for field.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessageView {
    pub message_type: u8,
    pub reserved0: [u8; 7],
    pub payload: MessageViewPayload,
}

/// `LnkClientVTable`, field for field and in the header's exact order — the order is the ABI.
#[repr(C)]
pub struct LnkClientVTable {
    pub vtable_bytes: u32,
    pub abi_version: u32,
    pub protocol_version: extern "C" fn() -> u32,
    pub protocol_fingerprint: extern "C" fn(out_fingerprint: *mut u8),
    pub connect: extern "C" fn(
        address_utf8: *const c_char,
        role: u8,
        timeout_milliseconds: u32,
        out_welcome: *mut Welcome,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
    pub poll: extern "C" fn(client: *mut LnkClient, out_message: *mut MessageView) -> LnkStatus,
    pub send_actions: extern "C" fn(client: *mut LnkClient, actions: *const Actions) -> LnkStatus,
    pub send_ping: extern "C" fn(client: *mut LnkClient, nonce: u64) -> LnkStatus,
    pub send_pong: extern "C" fn(client: *mut LnkClient, nonce: u64) -> LnkStatus,
    pub flush: extern "C" fn(client: *mut LnkClient, out_everything_left: *mut u8) -> LnkStatus,
    pub close: extern "C" fn(client: *mut LnkClient),
}

static VTABLE: LnkClientVTable = LnkClientVTable {
    vtable_bytes: size_of::<LnkClientVTable>() as u32,
    abi_version: LNK_CLIENT_ABI_VERSION,
    protocol_version: protocol_version_impl,
    protocol_fingerprint: protocol_fingerprint_impl,
    connect: connect_impl,
    poll: poll_impl,
    send_actions: send_actions_impl,
    send_ping: send_ping_impl,
    send_pong: send_pong_impl,
    flush: flush_impl,
    close: close_impl,
};

/// The library's one exported symbol. Kept camel-case to match the header and the flagship's
/// `tglGetProgramVTable` twin.
#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn lnkGetClientVTable(abi_version: u32) -> *const LnkClientVTable {
    if abi_version == LNK_CLIENT_ABI_VERSION {
        &raw const VTABLE
    } else {
        std::ptr::null()
    }
}

/// Every boundary function runs inside this: a panic becomes the fallback value instead of an
/// unwind into a foreign runtime, which would be undefined behaviour rather than an error path.
fn guarded<R>(fallback: R, run: impl FnOnce() -> R) -> R {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn status_of(error: &TransportError) -> LnkStatus {
    match error {
        TransportError::Io(_) => LNK_IO,
        TransportError::Frame(_) => LNK_FRAME_REFUSED,
        TransportError::Refused { .. } => LNK_REFUSED,
        TransportError::Garbled(_) => LNK_GARBLED,
        TransportError::PeerClosed => LNK_PEER_CLOSED,
        TransportError::HandshakeTimedOut => LNK_HANDSHAKE_TIMED_OUT,
    }
}

fn detail_of(error: &TransportError) -> String {
    match error {
        TransportError::Refused { reason } => reason.clone(),
        TransportError::Io(kind) => format!("link: io error: {kind:?}"),
        TransportError::Frame(refusal) => format!("link: the wire contract refused a frame: {refusal:?}"),
        TransportError::Garbled(expectation) => format!("link: garbled handshake: {expectation}"),
        TransportError::PeerClosed => "link: the peer closed the connection".to_string(),
        TransportError::HandshakeTimedOut => "link: the handshake timed out".to_string(),
    }
}

/// Copy a NUL-terminated, possibly truncated UTF-8 line into the caller's buffer. A null buffer
/// or zero capacity means the caller declined the words, which is its right.
fn write_detail(detail: *mut c_char, capacity: u32, text: &str) {
    if detail.is_null() || capacity == 0 {
        return;
    }
    let room = (capacity - 1) as usize;
    let bytes = text.as_bytes();
    let take = bytes.len().min(room);
    // SAFETY: the caller handed this buffer and its capacity as a pair; writing at most
    // capacity-1 bytes plus the terminator stays inside what it promised.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), detail.cast::<u8>(), take);
        *detail.add(take) = 0;
    }
}

extern "C" fn protocol_version_impl() -> u32 {
    guarded(0, || PROTOCOL_VERSION)
}

extern "C" fn protocol_fingerprint_impl(out_fingerprint: *mut u8) {
    guarded((), || {
        if out_fingerprint.is_null() {
            return;
        }
        let fingerprint = recorded_fingerprint();
        // SAFETY: the header declares the out parameter as uint8_t[32]; the caller owns those
        // bytes and asked for them to be filled.
        unsafe {
            std::ptr::copy_nonoverlapping(fingerprint.as_ptr(), out_fingerprint, fingerprint.len());
        }
    });
}

extern "C" fn connect_impl(
    address_utf8: *const c_char,
    role: u8,
    timeout_milliseconds: u32,
    out_welcome: *mut Welcome,
    out_status: *mut LnkStatus,
    out_detail_utf8: *mut c_char,
    detail_capacity_bytes: u32,
) -> *mut LnkClient {
    guarded(std::ptr::null_mut(), || {
        if out_status.is_null() {
            // Nowhere to say what went wrong: the only honest answer is nothing at all.
            return std::ptr::null_mut();
        }
        // Pre-set the panic verdict so an unwind caught by the guard leaves the truth behind.
        // SAFETY: out_status was just checked non-null and belongs to the caller.
        unsafe { *out_status = LNK_PANIC };

        let refuse = |status: LnkStatus, detail: &str| -> *mut LnkClient {
            // SAFETY: as above.
            unsafe { *out_status = status };
            write_detail(out_detail_utf8, detail_capacity_bytes, detail);
            std::ptr::null_mut()
        };

        if address_utf8.is_null() || out_welcome.is_null() {
            return refuse(LNK_BAD_ARGUMENT, "link: a null pointer is not an argument");
        }
        // SAFETY: the caller promised a NUL-terminated string; from_ptr reads to the NUL.
        let address = match unsafe { CStr::from_ptr(address_utf8) }.to_str() {
            Ok(address) => address,
            Err(_) => return refuse(LNK_BAD_ARGUMENT, "link: the address is not UTF-8"),
        };
        let role = match role {
            r if r == Role::Spectator as u8 => Role::Spectator,
            r if r == Role::CreatureHost as u8 => Role::CreatureHost,
            _ => return refuse(LNK_BAD_ARGUMENT, "link: role must be spectator (1) or creature host (2)"),
        };

        match connect(address, &local_hello(role), Duration::from_millis(u64::from(timeout_milliseconds))) {
            Ok((connection, welcome)) => {
                // SAFETY: out_welcome was checked non-null; Welcome is plain old data.
                unsafe {
                    *out_welcome = welcome;
                    *out_status = LNK_OK;
                }
                Box::into_raw(Box::new(ClientInner {
                    connection,
                    tick_rows: Vec::new(),
                }))
                .cast::<LnkClient>()
            }
            Err(error) => refuse(status_of(&error), &detail_of(&error)),
        }
    })
}

/// The one place a handle pointer becomes a Rust borrow again.
///
/// # Safety
///
/// `client` must be a pointer returned by [`connect_impl`] and not yet passed to
/// [`close_impl`] — which is exactly the contract the header states.
unsafe fn inner_of<'a>(client: *mut LnkClient) -> &'a mut ClientInner {
    // SAFETY: delegated to the caller, per the function's contract above.
    unsafe { &mut *client.cast::<ClientInner>() }
}

extern "C" fn poll_impl(client: *mut LnkClient, out_message: *mut MessageView) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if client.is_null() || out_message.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { inner_of(client) };

        let message = match inner.connection.poll() {
            Ok(None) => return LNK_NOTHING_YET,
            Ok(Some(message)) => message,
            Err(error) => return status_of(&error),
        };

        let (message_type, payload) = match message {
            Message::Hello(hello) => (MessageType::Hello, MessageViewPayload { hello }),
            Message::Welcome(welcome) => (MessageType::Welcome, MessageViewPayload { welcome }),
            Message::TickState { header, states } => {
                inner.tick_rows = states;
                (
                    MessageType::TickState,
                    MessageViewPayload {
                        tick_state: TickStateView {
                            header,
                            states: inner.tick_rows.as_ptr(),
                        },
                    },
                )
            }
            Message::Actions(actions) => (MessageType::Actions, MessageViewPayload { actions }),
            Message::Event(event) => (MessageType::Event, MessageViewPayload { event }),
            Message::Derez(derez) => (MessageType::Derez, MessageViewPayload { derez }),
            Message::Ping(ping) => (MessageType::Ping, MessageViewPayload { ping }),
            Message::Pong(pong) => (MessageType::Pong, MessageViewPayload { pong }),
            Message::Bye => (MessageType::Bye, MessageViewPayload { ping: Ping { nonce: 0 } }),
        };

        // SAFETY: out_message was checked non-null; MessageView is plain old data.
        unsafe {
            (*out_message).message_type = message_type as u8;
            (*out_message).reserved0 = [0; 7];
            (*out_message).payload = payload;
        }
        LNK_OK
    })
}

fn queue_on(client: *mut LnkClient, message: &Message) -> LnkStatus {
    if client.is_null() {
        return LNK_BAD_ARGUMENT;
    }
    // SAFETY: non-null was just checked; validity is the header's stated contract.
    let inner = unsafe { inner_of(client) };
    match inner.connection.queue(message) {
        Ok(()) => LNK_OK,
        Err(_) => LNK_BAD_ARGUMENT,
    }
}

extern "C" fn send_actions_impl(client: *mut LnkClient, actions: *const Actions) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if actions.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; Actions is plain old data, read by copy.
        let actions = unsafe { *actions };
        queue_on(client, &Message::Actions(actions))
    })
}

extern "C" fn send_ping_impl(client: *mut LnkClient, nonce: u64) -> LnkStatus {
    guarded(LNK_PANIC, || queue_on(client, &Message::Ping(Ping { nonce })))
}

extern "C" fn send_pong_impl(client: *mut LnkClient, nonce: u64) -> LnkStatus {
    guarded(LNK_PANIC, || queue_on(client, &Message::Pong(Pong { nonce })))
}

extern "C" fn flush_impl(client: *mut LnkClient, out_everything_left: *mut u8) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if client.is_null() || out_everything_left.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { inner_of(client) };
        match inner.connection.flush() {
            Ok(done) => {
                // SAFETY: out_everything_left was checked non-null.
                unsafe { *out_everything_left = u8::from(done) };
                LNK_OK
            }
            Err(error) => status_of(&error),
        }
    })
}

extern "C" fn close_impl(client: *mut LnkClient) {
    guarded((), || {
        if client.is_null() {
            return;
        }
        // SAFETY: the header's contract - a pointer from connect, not yet closed - and from
        // here the box owns it again, so it is freed exactly once.
        let inner = unsafe { Box::from_raw(client.cast::<ClientInner>()) };
        inner.connection.close();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Message;
    use crate::protocol::TickStateHeader;
    use crate::transport::accept;
    use std::ffi::CString;
    use std::net::TcpListener;

    const PATIENCE: Duration = Duration::from_secs(5);

    fn vtable() -> &'static LnkClientVTable {
        let table = lnkGetClientVTable(LNK_CLIENT_ABI_VERSION);
        assert!(!table.is_null());
        // SAFETY: the pointer is the static VTABLE, alive for the program's whole life.
        unsafe { &*table }
    }

    #[test]
    fn the_vtable_refuses_every_version_but_its_own() {
        assert!(lnkGetClientVTable(0).is_null());
        assert!(lnkGetClientVTable(LNK_CLIENT_ABI_VERSION + 1).is_null());
        let table = vtable();
        assert_eq!(table.vtable_bytes as usize, size_of::<LnkClientVTable>());
        assert_eq!(table.abi_version, LNK_CLIENT_ABI_VERSION);
        assert_eq!((table.protocol_version)(), PROTOCOL_VERSION);
        let mut fingerprint = [0u8; 32];
        (table.protocol_fingerprint)(fingerprint.as_mut_ptr());
        assert_eq!(fingerprint, recorded_fingerprint());
    }

    #[test]
    fn null_arguments_are_refused_not_dereferenced() {
        let table = vtable();
        let mut view = unsafe { std::mem::zeroed::<MessageView>() };
        assert_eq!((table.poll)(std::ptr::null_mut(), &raw mut view), LNK_BAD_ARGUMENT);
        assert_eq!((table.send_actions)(std::ptr::null_mut(), std::ptr::null()), LNK_BAD_ARGUMENT);
        assert_eq!((table.send_ping)(std::ptr::null_mut(), 1), LNK_BAD_ARGUMENT);
        let mut left = 0u8;
        assert_eq!((table.flush)(std::ptr::null_mut(), &raw mut left), LNK_BAD_ARGUMENT);
        (table.close)(std::ptr::null_mut());

        let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
        let mut status: LnkStatus = -1;
        let client = (table.connect)(std::ptr::null(), 2, 100, &raw mut welcome, &raw mut status, std::ptr::null_mut(), 0);
        assert!(client.is_null());
        assert_eq!(status, LNK_BAD_ARGUMENT);

        let address = CString::new("127.0.0.1:1").expect("address literal");
        let client = (table.connect)(address.as_ptr(), 7, 100, &raw mut welcome, &raw mut status, std::ptr::null_mut(), 0);
        assert!(client.is_null());
        assert_eq!(status, LNK_BAD_ARGUMENT, "a role that is not a role is refused before any socket");
    }

    #[test]
    fn a_c_shaped_caller_gets_the_whole_conversation() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
        let address = listener.local_addr().expect("address");

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let (mut connection, hello) = accept(stream, PATIENCE).expect("handshake");
            assert_eq!(hello.role, Role::CreatureHost as u8);

            connection
                .queue(&Message::Welcome(Welcome {
                    current_tick: 100,
                    nominal_dt_seconds: 0.03125,
                    client_id: 9,
                }))
                .expect("queue WELCOME");
            let rows = vec![
                CreatureState {
                    creature_id: 0,
                    position: [0.0, 1.0, 2.0],
                    yaw: 0.1,
                    velocity: [0.0, 0.0, 0.0],
                    yaw_rate: 0.0,
                    vocalisation: 0.0,
                },
                CreatureState {
                    creature_id: 1,
                    position: [3.0, 4.0, 5.0],
                    yaw: 0.2,
                    velocity: [1.0, 0.0, 0.0],
                    yaw_rate: 0.5,
                    vocalisation: 0.75,
                },
            ];
            connection
                .queue(&Message::TickState {
                    header: TickStateHeader {
                        tick: 100,
                        creature_count: 2,
                        reserved0: [0; 4],
                    },
                    states: rows,
                })
                .expect("queue TICK_STATE");
            assert!(connection.flush().expect("flush"));

            // Now the client's turn: ACTIONS then PING, answered with a PONG.
            let deadline = std::time::Instant::now() + PATIENCE;
            let mut heard_actions = false;
            loop {
                match connection.poll().expect("server poll") {
                    Some(Message::Actions(actions)) => {
                        assert_eq!(actions.tick, 101);
                        assert_eq!(actions.creature_id, 1);
                        heard_actions = true;
                    }
                    Some(Message::Ping(ping)) => {
                        assert!(heard_actions, "coalesced order: ACTIONS was queued first");
                        connection.queue(&Message::Pong(Pong { nonce: ping.nonce })).expect("queue PONG");
                        assert!(connection.flush().expect("flush PONG"));
                        break;
                    }
                    Some(other) => panic!("unexpected {other:?}"),
                    None => {
                        assert!(std::time::Instant::now() < deadline, "client frames never arrived");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            }
        });

        let table = vtable();
        let address = CString::new(address.to_string()).expect("address string");
        let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
        let mut status: LnkStatus = -1;
        let mut detail = [0i8; 256];
        let client = (table.connect)(
            address.as_ptr(),
            Role::CreatureHost as u8,
            5_000,
            &raw mut welcome,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        assert_eq!(status, LNK_OK);
        assert!(!client.is_null());
        assert_eq!(welcome.current_tick, 100);
        assert_eq!(welcome.client_id, 9);

        let mut view = unsafe { std::mem::zeroed::<MessageView>() };
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            match (table.poll)(client, &raw mut view) {
                LNK_NOTHING_YET => {
                    assert!(std::time::Instant::now() < deadline, "TICK_STATE never arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                LNK_OK => break,
                other => panic!("poll answered {other}"),
            }
        }
        assert_eq!(view.message_type, MessageType::TickState as u8);
        // SAFETY: the type byte names the union member, and the rows stay valid until the next
        // poll — the exact contract the header states for the C side.
        let (header, second_row) = unsafe {
            let tick_state = view.payload.tick_state;
            (tick_state.header, *tick_state.states.add(1))
        };
        assert_eq!(header.tick, 100);
        assert_eq!(header.creature_count, 2);
        assert_eq!(second_row.creature_id, 1);
        assert_eq!(second_row.vocalisation, 0.75);

        let actions = Actions {
            tick: 101,
            creature_id: 1,
            desired_forward_speed: 1.5,
            desired_turn_rate: -0.25,
            vocalisation_strength: 0.0,
        };
        assert_eq!((table.send_actions)(client, &raw const actions), LNK_OK);
        assert_eq!((table.send_ping)(client, 0xC0FFEE), LNK_OK);
        let mut everything_left = 0u8;
        assert_eq!((table.flush)(client, &raw mut everything_left), LNK_OK);
        assert_eq!(everything_left, 1);

        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            match (table.poll)(client, &raw mut view) {
                LNK_NOTHING_YET => {
                    assert!(std::time::Instant::now() < deadline, "PONG never arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                LNK_OK => break,
                other => panic!("poll answered {other}"),
            }
        }
        assert_eq!(view.message_type, MessageType::Pong as u8);
        // SAFETY: the type byte names the union member.
        assert_eq!(unsafe { view.payload.pong.nonce }, 0xC0FFEE);

        (table.close)(client);
        server.join().expect("server thread");
    }

    #[test]
    fn a_refusal_reaches_the_c_caller_in_words() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
        let address = listener.local_addr().expect("address");

        let server = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut stream, _) = listener.accept().expect("accept");
            let mut opening = [0u8; 4 + 3 + 40];
            stream.read_exact(&mut opening).expect("magic and HELLO");
            stream.write_all(b"link: no vacancy tonight\n").expect("refusal");
        });

        let table = vtable();
        let address = CString::new(address.to_string()).expect("address string");
        let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
        let mut status: LnkStatus = -1;
        let mut detail = [0i8; 256];
        let client = (table.connect)(
            address.as_ptr(),
            Role::Spectator as u8,
            5_000,
            &raw mut welcome,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        assert!(client.is_null());
        assert_eq!(status, LNK_REFUSED);
        let words = unsafe { CStr::from_ptr(detail.as_ptr().cast::<c_char>()) }.to_string_lossy().to_string();
        assert!(words.contains("no vacancy"), "the refusal must arrive verbatim, got: {words}");
        server.join().expect("server thread");
    }

    /// The cross-language twin check without a C compiler: the header's #define lines are
    /// parsed out of the header itself, which is compiled into this test, and each one must
    /// equal the Rust constant it mirrors. A drift refuses here, red, immediately.
    #[test]
    fn the_header_and_the_rust_constants_are_the_same_constants() {
        let header = include_str!("../include/lnk/lnk_client.h");
        let pins: &[(&str, i64)] = &[
            ("LNK_CLIENT_ABI_VERSION", i64::from(LNK_CLIENT_ABI_VERSION)),
            ("LNK_OK", i64::from(LNK_OK)),
            ("LNK_NOTHING_YET", i64::from(LNK_NOTHING_YET)),
            ("LNK_REFUSED", i64::from(LNK_REFUSED)),
            ("LNK_HANDSHAKE_TIMED_OUT", i64::from(LNK_HANDSHAKE_TIMED_OUT)),
            ("LNK_PEER_CLOSED", i64::from(LNK_PEER_CLOSED)),
            ("LNK_FRAME_REFUSED", i64::from(LNK_FRAME_REFUSED)),
            ("LNK_GARBLED", i64::from(LNK_GARBLED)),
            ("LNK_IO", i64::from(LNK_IO)),
            ("LNK_BAD_ARGUMENT", i64::from(LNK_BAD_ARGUMENT)),
            ("LNK_PANIC", i64::from(LNK_PANIC)),
        ];
        for (name, value) in pins {
            let defined = header
                .lines()
                .find_map(|line| {
                    let rest = line.strip_prefix("#define ")?.strip_prefix(name)?.trim();
                    (!rest.is_empty() && !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
                        .then_some(rest)
                        .or(Some(rest))
                        .filter(|_| line.split_whitespace().nth(1) == Some(name))
                })
                .unwrap_or_else(|| panic!("{name} is not defined in lnk_client.h"));
            let digits: String = defined.chars().take_while(|c| c.is_ascii_digit()).collect();
            assert_eq!(
                digits.parse::<i64>().expect("a numeric define"),
                *value,
                "{name} drifted between the header and the Rust side"
            );
        }
    }
}
