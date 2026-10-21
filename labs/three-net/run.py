#!/usr/bin/env python3
"""Real twelve-process (10 users + two voluntary gateways) Roha QUIC lab."""
import argparse
import json
import os
import subprocess
import time
from pathlib import Path

NODES = [f"peer{i:02d}" for i in range(1, 11)] + ["gateab", "gatebc"]
PROJECT = f"akula-three-net-{os.getpid()}"
ROOT = Path(__file__).resolve().parents[2]
BASE = ["docker", "compose", "-f", str(Path(__file__).with_name("compose.yaml")),
        "-p", PROJECT]


def process(cmd, *, timeout=180, ok=True, cwd=None):
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, cwd=cwd)
    if ok and p.returncode:
        raise RuntimeError(f"{cmd}: {p.stdout[-2500:]} {p.stderr[-2500:]}")
    return p


def dc(*args, ok=True, timeout=180):
    return process([*BASE, *args], ok=ok, timeout=timeout)


def node(name, *args, ok=True):
    return dc("exec", "-T", name, "/app/lab", *args, ok=ok, timeout=30)


def value(name, *args):
    return node(name, *args).stdout.strip()


def guard(name, expected):
    actual = int(value(name, "count"))
    assert actual == expected, (name, expected, actual)


def await_node(name):
    for _ in range(40):
        if node(name, "id", ok=False).returncode == 0:
            return
        time.sleep(1)
    raise AssertionError(f"{name} unavailable")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--output", default="labs/three-net/report.json")
    args = ap.parse_args()
    report = {"status": "FAIL", "network": "three isolated Docker networks",
              "user_nodes": 10, "voluntary_gateways": 2, "checks": []}
    try:
        build = process(["docker", "build", "--progress=plain", "-t", "akula-three-net:local",
                         "-f", "labs/three-net/Dockerfile", "."], cwd=ROOT, timeout=2400,
                        ok=False)
        if build.returncode:
            raise RuntimeError("build:\n" + build.stdout[-9000:] + build.stderr[-9000:])
        dc("up", "-d", "--no-build", "--wait", timeout=240)
        keys = {name: value(name, "id") for name in NODES}
        assert len(set(keys.values())) == 12
        report["checks"].append("twelve separate identities, no published ports")

        # Gateway A accepts peer01, gateway B accepts gateway A; only peer10
        # may fetch its envelope from gateway B. All enrollment is explicit.
        assert value("gateab", "allow", keys["peer01"]) == "ALLOWED"
        assert value("gatebc", "allow", keys["gateab"]) == "ALLOWED"
        assert value("gatebc", "allow", keys["peer10"]) == "ALLOWED"

        # Peer01 has NO direct DNS/path to the peer10 container in gamma.
        blocked = node("peer01", "send", "peer10", keys["peer10"],
                       "direct-disallowed", "must-not-pass", ok=False)
        assert blocked.returncode != 0
        report["checks"].append("alpha to gamma direct contact blocked")

        # Store while recipient is down. This is NOT a final recipient ACK.
        dc("stop", "peer10")
        assert value("peer01", "relay-send", "gateab", keys["gateab"],
                     keys["peer10"], "offline-one", "hello across three networks") == "ACK:offline-one:STORED"
        assert value("gateab", "forward", "gatebc", keys["gatebc"],
                     "offline-one") == "ACK:offline-one:STORED"
        report["checks"].append("signed envelope stored via two real QUIC hops while recipient offline")

        # Wrong recipient's key cannot pull the third-party envelope.
        assert value("gatebc", "allow", keys["peer08"]) == "ALLOWED"
        assert value("peer08", "pull", "gatebc", keys["gatebc"]) == "PULLED:0"
        guard("peer08", 0)
        report["checks"].append("unrelated peer cannot fetch recipient's envelope")

        dc("start", "peer10")
        await_node("peer10")
        assert value("peer10", "id") == keys["peer10"]
        assert value("peer10", "pull", "gatebc", keys["gatebc"]) == "PULLED:1"
        guard("peer10", 1)
        # Idempotency after a recipient and gateway restart.
        dc("restart", "gatebc")
        await_node("gatebc")
        assert value("gatebc", "id") == keys["gatebc"]
        assert value("peer10", "pull", "gatebc", keys["gatebc"]) == "PULLED:0"
        guard("peer10", 1)
        report["checks"].append("offline recipient retrieves authenticated message; idempotent restart")

        # Cross-network sender cannot reach B gateway without A gateway's
        # voluntary forwarding role.
        dc("stop", "gateab")
        offline = node("peer01", "relay-send", "gateab", keys["gateab"],
                       keys["peer10"], "blocked-gateway", "should fail", ok=False)
        assert offline.returncode != 0
        guard("peer10", 1)
        report["checks"].append("gateway failure blocks path without false recipient acknowledgement")

        report["status"] = "PASS"
    except Exception as exc:
        report["error"] = repr(exc)
        log = dc("logs", "--no-color", "--tail", "60", ok=False)
        report["container_logs"] = (log.stdout + log.stderr)[-16000:]
        raise
    finally:
        out = ROOT / args.output
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
        dc("down", "-v", "--remove-orphans", ok=False)
        print(json.dumps(report, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
