#!/usr/bin/env python3
"""Plan 181 §4.2 independent IRC client driver (unmodified jaraco/irc).

Drives registration/join/message behavior exclusively through the
public `irc.client` API against the i2pr IRC client tunnel. No raw
sockets, no vendored protocol code, no source patching: the
interpreter runs from a venv installed from the exact-pinned
jaraco/irc checkout verified by
scripts/interop/fetch-service-tunnel-clients.sh.

Emits KEY=value facts on stdout.
"""

import argparse
import sys
import threading
import time

import irc.client


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--nick", default="alice")
    parser.add_argument("--channel", default="#chan")
    args = parser.parse_args()

    facts = {}
    events = {"welcome": threading.Event(), "echo": threading.Event()}

    def on_welcome(connection, event):
        facts["WELCOME"] = "1"
        events["welcome"].set()

    def on_pubmsg(connection, event):
        text = event.arguments[0] if event.arguments else ""
        if "echo-hello" in text:
            facts["ECHO_RECEIVED"] = "1"
            events["echo"].set()

    def on_ping(connection, event):
        token = event.arguments[0] if event.arguments else "x"
        try:
            connection.pong(token)
            facts["PONG_SENT"] = "1"
        except Exception:
            facts["PONG_SENT"] = "0"

    def on_disconnect(connection, event):
        facts["DISCONNECTED"] = "1"

    reactor = irc.client.Reactor()
    try:
        connection = reactor.server().connect(args.host, args.port, args.nick)
    except irc.client.ServerConnectionError as error:
        print(f"CONNECT_FAILED={error}")
        return 0
    facts["CONNECTED"] = "1"
    connection.add_global_handler("welcome", on_welcome)
    connection.add_global_handler("pubmsg", on_pubmsg)
    connection.add_global_handler("ping", on_ping)
    connection.add_global_handler("disconnect", on_disconnect)

    worker = threading.Thread(target=reactor.process_forever, kwargs={"timeout": 0.2})
    worker.daemon = True
    worker.start()
    try:
        connection.nick(args.nick)
        connection.user("alice", "Alice")
        facts["REGISTER_SENT"] = "1"
        # CAP where the library exposes it; best-effort only.
        try:
            cap = getattr(connection, "cap", None)
            if callable(cap):
                cap("LS")
                facts["CAP_SENT"] = "1"
            else:
                facts["CAP_SENT"] = "unexposed"
        except Exception:
            facts["CAP_SENT"] = "0"
        if not events["welcome"].wait(timeout=15):
            facts["WELCOME"] = "0"
        connection.join(args.channel)
        facts["JOIN_SENT"] = "1"
        time.sleep(0.5)
        connection.privmsg(args.channel, "hello world")
        facts["PRIVMSG_SENT"] = "1"
        if not events["echo"].wait(timeout=15):
            facts["ECHO_RECEIVED"] = facts.get("ECHO_RECEIVED", "0")
        # CTCP ACTION must pass the privacy filter; DCC must be
        # dropped by it. Both go through the public ctcp API.
        connection.action(args.channel, "waves hello")
        facts["ACTION_SENT"] = "1"
        time.sleep(0.5)
        connection.privmsg(args.channel, "\x01DCC SEND file 127.0.0.1 0 1024\x01")
        facts["DCC_SENT"] = "1"
        time.sleep(1.0)
        connection.quit("done")
        facts["QUIT_SENT"] = "1"
        time.sleep(1.0)
    finally:
        try:
            reactor.disconnect_all()
        except Exception:
            pass
    for key in (
        "CONNECTED",
        "REGISTER_SENT",
        "CAP_SENT",
        "WELCOME",
        "JOIN_SENT",
        "PRIVMSG_SENT",
        "ECHO_RECEIVED",
        "PONG_SENT",
        "ACTION_SENT",
        "DCC_SENT",
        "QUIT_SENT",
    ):
        print(f"{key}={facts.get(key, '0')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
