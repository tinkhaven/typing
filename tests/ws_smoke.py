"""End-to-end check of the practice socket: honest run, publish, and rejection."""
import asyncio, json, os, sys
import websockets

# Defaults to a local server; point it anywhere with TYPING_WS_URL, e.g.
#   TYPING_WS_URL=wss://typing.example.com/api/ws python3 tests/ws_smoke.py
URL = os.environ.get("TYPING_WS_URL", "ws://127.0.0.1:8080/api/ws")
ok, fail = [], []

def check(name, cond, detail=""):
    (ok if cond else fail).append(name)
    print(f"  {'PASS' if cond else 'FAIL'}  {name}{'  ' + detail if detail else ''}")

async def recv(ws, want=None, timeout=5):
    while True:
        msg = json.loads(await asyncio.wait_for(ws.recv(), timeout))
        if want is None or msg["type"] == want:
            return msg
        if msg["type"] == "problem":
            return msg

async def stream(ws, count, dt_us, kind="correct", batch=20):
    sent = 0
    while sent < count:
        n = min(batch, count - sent)
        await ws.send(json.dumps({
            "type": "touches",
            "touches": [{"kind": kind, "dt_us": dt_us} for _ in range(n)],
        }))
        sent += n

async def honest_run(module, language, target_wpm=200, nickname="Tester"):
    async with websockets.connect(URL) as ws:
        await ws.send(json.dumps({
            "type": "start", "module": module, "layout": "qwerty_us",
            "language": language, "lesson": None, "seed": 20260901,
        }))
        following = await recv(ws, "following")
        if following["type"] != "following":
            return None, following
        chars = following["expected_chars"]

        # Pace the run so the reported speed lands near target_wpm.
        seconds = 12.0 * chars / target_wpm
        dt_us = max(1, int(seconds * 1_000_000 / chars))
        await stream(ws, chars, dt_us)

        elapsed = chars * dt_us
        await ws.send(json.dumps({
            "type": "finish",
            "client_session": {
                "touches": chars, "errors": 0,
                "intervals_us": [dt_us] * (chars - 1),
                "elapsed_us": elapsed,
            },
        }))
        scored = await recv(ws, "scored")
        if scored["type"] != "scored":
            return chars, scored
        board = None
        if scored.get("publishable"):
            await ws.send(json.dumps({"type": "publish", "nickname": nickname}))
            board = await recv(ws, "board")
        return chars, (scored, board)

async def main():
    print(f"target: {URL}\n")
    print("1. Velocity: honest run at ~200 wpm, then publish")
    chars, result = await honest_run("velocity", "en_GB", nickname="Dieter")
    check("velocity exercise reproduced by the server", chars and chars > 300, f"{chars} chars")
    if isinstance(result, tuple):
        scored, board = result
        s = scored["score"]
        check("accuracy is 100%", abs(s["accuracy"] - 100.0) < 0.01, f"{s['accuracy']:.1f}%")
        check("speed near the 200 wpm target", 180 < s["speed"] < 220, f"{s['speed']:.1f} wpm")
        check("goals met", scored["goals_met"] is True)
        check("run is publishable (the ~470-char floor bug)", scored["publishable"] is True)
        check("a rank was offered", scored["would_rank"] == 1, str(scored["would_rank"]))
        check("board came back after publishing", board is not None and board["type"] == "board")
        if board and board["type"] == "board":
            e = board["entries"]
            # The board may already hold rows from an earlier run of this script,
            # so check for presence and ordering rather than an exact length.
            check("board lists the published entry", any(r["nickname"] == "Dieter" for r in e), str(e[:2]))
            check("board is ordered fastest first",
                  all(e[i]["speed"] >= e[i + 1]["speed"] for i in range(len(e) - 1)),
                  str([r["speed"] for r in e]))
            check("board ranks are sequential from 1",
                  [r["rank"] for r in e] == list(range(1, len(e) + 1)), str([r["rank"] for r in e]))
    else:
        check("velocity scored", False, json.dumps(result))

    print("\n2. Fluidness: honest run (needs 500+ chars for the board)")
    chars, result = await honest_run("fluidness", "en_GB", target_wpm=120, nickname="Fluent")
    if isinstance(result, tuple):
        scored, board = result
        s = scored["score"]
        check("fluidness exercise is long enough", chars >= 500, f"{chars} chars")
        check("a fluidness figure was produced", s["fluidness"] is not None, str(s["fluidness"]))
        check("perfectly even rhythm scores ~100%", s["fluidness"] and s["fluidness"] > 99.0)
        check("fluidness run is publishable", scored["publishable"] is True)
    else:
        check("fluidness scored", False, json.dumps(result))

    print("\n3. Rejection: superhuman speed")
    async with websockets.connect(URL) as ws:
        await ws.send(json.dumps({"type": "start", "module": "velocity", "layout": "qwerty_us",
                                  "language": "en_GB", "lesson": None, "seed": 7}))
        f = await recv(ws, "following")
        chars = f["expected_chars"]
        await stream(ws, chars, 1_000)  # 1 ms per key
        await ws.send(json.dumps({"type": "finish", "client_session": {
            "touches": chars, "errors": 0, "intervals_us": [], "elapsed_us": chars * 1000}}))
        msg = await recv(ws)
        check("superhuman run refused", msg["type"] == "problem" and msg["code"] == "implausible",
              json.dumps(msg)[:120])

    print("\n4. Rejection: unfinished run claimed as complete")
    async with websockets.connect(URL) as ws:
        await ws.send(json.dumps({"type": "start", "module": "velocity", "layout": "qwerty_us",
                                  "language": "en_GB", "lesson": None, "seed": 8}))
        f = await recv(ws, "following")
        await stream(ws, 50, 200_000)
        await ws.send(json.dumps({"type": "finish", "client_session": {
            "touches": 50, "errors": 0, "intervals_us": [], "elapsed_us": 10_000_000}}))
        msg = await recv(ws)
        check("unfinished run refused", msg["type"] == "problem" and msg["code"] == "implausible",
              json.dumps(msg)[:120])

    print("\n5. Bad input handling")
    async with websockets.connect(URL) as ws:
        await ws.send(json.dumps({"type": "start", "module": "velocity", "layout": "no_such_kbd",
                                  "language": "en_GB", "lesson": None, "seed": 1}))
        msg = await recv(ws)
        check("unknown layout refused", msg.get("code") == "unknown-layout", json.dumps(msg)[:100])

        await ws.send(json.dumps({"type": "start", "module": "velocity", "layout": "qwerty_us",
                                  "language": "klingon", "lesson": None, "seed": 1}))
        msg = await recv(ws)
        check("unknown language refused", msg.get("code") == "unknown-language", json.dumps(msg)[:100])

        await ws.send(json.dumps({"type": "touches", "touches": [{"kind": "correct", "dt_us": 1}]}))
        msg = await recv(ws)
        check("keystrokes before a start refused", msg.get("code") == "out-of-order", json.dumps(msg)[:100])

        await ws.send(json.dumps({"type": "finish", "client_session": {
            "touches": 1, "errors": 0, "intervals_us": [], "elapsed_us": 1}}))
        msg = await recv(ws)
        check("finish before a start refused", msg.get("code") == "out-of-order", json.dumps(msg)[:100])

        await ws.send(json.dumps({"type": "publish", "nickname": "Nobody"}))
        msg = await recv(ws)
        check("publish with no result refused", msg.get("code") == "out-of-order", json.dumps(msg)[:100])

        await ws.send(json.dumps({"type": "ping"}))
        msg = await recv(ws, "pong")
        check("ping answered", msg["type"] == "pong")

        await ws.send("this is not json")
        msg = await recv(ws)
        check("malformed frame does not kill the socket", msg["type"] == "problem")
        await ws.send(json.dumps({"type": "ping"}))
        msg = await recv(ws, "pong")
        check("socket still usable afterwards", msg["type"] == "pong")

    print(f"\n{len(ok)} passed, {len(fail)} failed")
    if fail:
        print("failed:", ", ".join(fail))
    return 1 if fail else 0

sys.exit(asyncio.run(main()))
