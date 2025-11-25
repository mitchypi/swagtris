import argparse
import json
import subprocess
import sys
import time
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description="Send minimal TBP messages to a bot for manual testing.")
    parser.add_argument(
        "--bot-path",
        default=Path("cold-clear-2/target/release/cold-clear-2.exe"),
        type=Path,
        help="Path to the bot executable (default: cold-clear-2 release build)",
    )
    parser.add_argument(
        "--queue",
        default="I,O,T",
        help="Comma-separated queue to send in start (default: I,O,T)",
    )
    parser.add_argument(
        "--board-rows",
        type=int,
        default=40,
        help="Number of board rows to send (default: 40)",
    )
    parser.add_argument(
        "--board-cols",
        type=int,
        default=10,
        help="Number of board columns to send (default: 10)",
    )
    parser.add_argument(
        "--no-stop",
        action="store_true",
        help="Do not send stop/quit at the end (useful if chaining multiple probes)",
    )
    parser.add_argument(
        "--suggest-delay",
        type=float,
        default=0.025,
        help="Seconds to wait after start before first suggest (default: 0.025)",
    )
    args = parser.parse_args()

    bot_path = args.bot_path
    if not bot_path.exists():
        print(f"Bot not found at {bot_path}", file=sys.stderr)
        return 1

    queue = [p.strip().upper() for p in args.queue.split(",") if p.strip()]
    board = [[None for _ in range(args.board_cols)] for _ in range(args.board_rows)]

    bot = subprocess.Popen(
        [str(bot_path)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )

    def send(obj):
        line = json.dumps(obj)
        bot.stdin.write(line + "\n")
        bot.stdin.flush()
        print("TX", line)

    def recv():
        line = bot.stdout.readline()
        if not line:
            print("RX <eof>")
            return None
        print("RX", line.strip())
        return line

    try:
        # Expect info first
        recv()
        # Rules -> expect ready
        send({"type": "rules"})
        # Drain until we see ready (or EOF)
        while True:
            line = recv()
            if not line or '"ready"' in line:
                break
        # Start from empty board
        send(
            {
                "type": "start",
                "board": board,
                "queue": queue,
                "hold": None,
                "combo": 0,
                "back_to_back": False,
            }
        )
        if args.suggest_delay > 0:
            time.sleep(args.suggest_delay)
        # Suggest after start
        send({"type": "suggest"})
        reply = recv()

        def parse_first_move(line):
            if not line:
                return None
            try:
                parsed = json.loads(line)
                moves = parsed.get("moves") or []
                return moves[0] if moves else None
            except Exception:
                return None

        first_move = parse_first_move(reply)
        if first_move is None and reply and '"moves":[]' in reply:
            send({"type": "suggest"})
            reply = recv()
            first_move = parse_first_move(reply)

        if first_move:
            send({"type": "play", "move": first_move})
            next_piece = queue[1] if len(queue) > 1 else queue[0]
            send({"type": "new_piece", "piece": next_piece})
            send({"type": "suggest"})
            recv()
        if not args.no_stop:
            send({"type": "stop"})
            send({"type": "quit"})
    finally:
        try:
            bot.stdin.close()
        except Exception:
            pass
        bot.wait(timeout=5)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
