/*
    Link - the wire of the Grid. The client surface of the loadable library.

    This header declares what a consumer that LoadLibrary()s or dlopen()s Link can call: one
    exported symbol, lnkGetClientVTable, returning a table of functions behind which a client
    connection lives. It deliberately mirrors the flagship's tglGetProgramVTable shape - a
    version asked for, NULL returned when the library cannot satisfy it, and the table's own
    size as its first member - because that refusal has already proven itself there.

    This header is the API, not the wire. The wire contract - message layouts, framing, the
    fingerprint - is lnk_protocol.h, fingerprinted and versioned separately, and included here
    because the API speaks in its types. The API may grow without the wire moving; the runtime
    check below is what keeps a stale copy of THIS header from lying about the library beside
    it.

    Copyright (C) 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/link>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

#ifndef LNK_CLIENT_H
#define LNK_CLIENT_H

#include <stdint.h>

#include "lnk_protocol.h"

#ifdef __cplusplus
extern "C"
{
#endif

/*! Bumped whenever this table or its rules change. lnkGetClientVTable refuses any other
    number, so a consumer built against a stale copy of this header is told at load time
    rather than corrupted at call time. */
#define LNK_CLIENT_ABI_VERSION 1u

/*
    Statuses. Zero is success and only success; everything else names its failure. No function
    behind the vtable ever unwinds, throws or aborts across this boundary - a panic inside the
    library is caught at the edge and reported as LNK_PANIC, the noexcept doctrine in library
    clothing.
*/
typedef int32_t LnkStatus;

#define LNK_OK 0
/*! poll: the socket holds no complete frame yet. Not an error; turn the loop and ask again. */
#define LNK_NOTHING_YET 1
/*! The far end said no during the handshake; the detail buffer carries its words verbatim. */
#define LNK_REFUSED 2
#define LNK_HANDSHAKE_TIMED_OUT 3
#define LNK_PEER_CLOSED 4
/*! The peer sent something the wire contract refuses. The connection is over: a peer that
    framed one message wrongly will frame the next wrongly too. */
#define LNK_FRAME_REFUSED 5
/*! The handshake bytes were not the handshake. */
#define LNK_GARBLED 6
#define LNK_IO 7
/*! A null pointer, an unreadable string, a role that is not a role. The library refuses the
    call rather than dereferencing hope. */
#define LNK_BAD_ARGUMENT 8
/*! The library caught its own panic at the boundary. The connection is not to be trusted
    afterwards; close it. */
#define LNK_PANIC 9

/*! A connected client, opaque. Everything it owns dies with close(). */
typedef struct LnkClient LnkClient;

/*! A received TICK_STATE: the header by value, the rows by borrow. The rows live in the
    library and stay valid until the next poll() or close() on the same client - copy them out
    if they must outlive that, exactly as the Program ABI's borrow rules already taught. */
typedef struct LnkTickStateView {
    LnkTickStateHeader header;
    const LnkCreatureState* states; /*!< creature_count rows, library-owned. */
} LnkTickStateView;

/*! One received message. `type` is the LNK_MSG_* that arrived and names the union member to
    read; BYE carries nothing, so the type alone says it all. This struct is API, not wire -
    it holds a pointer, so its size is the platform's business and nothing pins it. */
typedef struct LnkMessageView {
    uint8_t type;
    uint8_t reserved0[7];
    union {
        LnkWelcome welcome;
        LnkTickStateView tick_state;
        LnkEvent event;
        LnkDerez derez;
        LnkPing ping;
        LnkPong pong;
        LnkHello hello;     /*!< Never sent by an honest server; the view does not judge. */
        LnkActions actions; /*!< Likewise. The consumer decides what arriving here means. */
    } as;
} LnkMessageView;

/*! The client surface. First two members mirror the Program ABI's vtable header: the size the
    library was compiled with, then the version, so a consumer checks both before calling
    anything. */
typedef struct LnkClientVTable {
    uint32_t vtable_bytes; /*!< sizeof(LnkClientVTable) as the library was compiled. */
    uint32_t abi_version;  /*!< LNK_CLIENT_ABI_VERSION as the library was compiled. */

    /*! LNK_PROTOCOL_VERSION as the library was compiled - the wire's version, for logs. */
    uint32_t (*protocol_version)(void);

    /*! The fingerprint this library's HELLO carries, for logs and for curiosity. */
    void (*protocol_fingerprint)(uint8_t out_fingerprint[32]);

    /*! The whole handshake: magic, HELLO with this library's own fingerprint, WELCOME back.
        Blocking, bounded by the timeout per read. On success returns the client and writes the
        WELCOME. On failure returns NULL, writes the status, and writes a NUL-terminated UTF-8
        line into the detail buffer - the server's refusal verbatim when there is one. `role`
        is LNK_ROLE_SPECTATOR or LNK_ROLE_CREATURE_HOST; `address_utf8` is host:port. */
    LnkClient* (*connect)(const char* address_utf8, uint8_t role, uint32_t timeout_milliseconds, LnkWelcome* out_welcome, LnkStatus* out_status,
                          char* out_detail_utf8, uint32_t detail_capacity_bytes);

    /*! One complete message if the socket holds one (LNK_OK, view written), LNK_NOTHING_YET if
        it does not. Never blocks. Any other status ends the connection's useful life. */
    LnkStatus (*poll)(LnkClient* client, LnkMessageView* out_message);

    /*! Stage the creature's intent for the next flush. The address and the twelve bytes,
        exactly as ACTIONS on the wire. */
    LnkStatus (*send_actions)(LnkClient* client, const LnkActions* actions);

    /*! Stage a PING carrying the nonce; the answering PONG arrives through poll(). */
    LnkStatus (*send_ping)(LnkClient* client, uint64_t nonce);

    /*! Stage the PONG answering a received PING, same nonce back. */
    LnkStatus (*send_pong)(LnkClient* client, uint64_t nonce);

    /*! One coalesced write per tick: push everything staged. Writes 1 to out_everything_left
        when the buffer emptied, 0 when the socket filled and the remainder is carried - call
        again next tick. */
    LnkStatus (*flush)(LnkClient* client, uint8_t* out_everything_left);

    /*! Say BYE, close the socket, free everything the client owns. The pointer is dead after
        this call; a NULL is ignored. */
    void (*close)(LnkClient* client);
} LnkClientVTable;

/*! The library's one exported symbol. Returns the table when it can satisfy `abi_version`,
    NULL when it cannot - refusal, not negotiation, exactly as the Program ABI behaves. */
const LnkClientVTable* lnkGetClientVTable(uint32_t abi_version);

typedef const LnkClientVTable* (*LnkGetClientVTableFn)(uint32_t abi_version);

#ifdef __cplusplus
}
#endif

#endif /* LNK_CLIENT_H */
