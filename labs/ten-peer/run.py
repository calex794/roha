#!/usr/bin/env python3
"""Roha 10-peer reproducible Docker lab. Requires Docker Compose v2."""
import argparse
import json
import math
import os
import subprocess
import time
from pathlib import Path

NODES = [f"peer{i:02d}" for i in range(1, 11)]
COMPOSE = Path(__file__).with_name("compose.yaml")
PROJECT = f"akula-lab-{os.getpid()}"
BASE = ["docker", "compose", "-f", str(COMPOSE), "-p", PROJECT]


def run(*args, ok=True, timeout=120):
    result = subprocess.run([*BASE, *args], capture_output=True, text=True, timeout=timeout)
    if ok and result.returncode:
        raise RuntimeError(f"{' '.join(args)}: {result.stderr[-12000:]}")
    return result


def exec_node(node, *args, ok=True):
    return run("exec", "-T", node, "/app/lab", *args, ok=ok, timeout=25)


def result(node, *args):
    return exec_node(node, *args).stdout.strip()


def assert_count(node, expected):
    actual = int(result(node, "count"))
    assert actual == expected, f"{node}: expected {expected}, got {actual}"


def wait_healthy(node, seconds=50):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if exec_node(node, "id", ok=False).returncode == 0:
            return
        time.sleep(1)
    raise AssertionError(f"{node} did not become healthy")


def send(sender, target, keys, message_id, body, *, target_key=None, expect_ok=True):
    start = time.monotonic()
    cmd = exec_node(sender, "send", target,
                    target_key or keys[target], message_id, body, ok=False)
    duration = (time.monotonic() - start) * 1000
    if expect_ok:
        assert cmd.returncode == 0, f"{sender} -> {target}: {cmd.stderr}"
        assert cmd.stdout.strip().startswith(f"ACK:{message_id}:"), cmd.stdout
    else:
        assert cmd.returncode != 0, f"unexpected ACK: {cmd.stdout}"
    return duration


def percentile(values, pct):
    ordered = sorted(values)
    return round(ordered[max(0, math.ceil(len(ordered) * pct / 100) - 1)], 1)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", default="labs/ten-peer/report.json")
    parser.add_argument("--keep", action="store_true",
                        help="Do not remove isolated test volumes and containers")
    opts = parser.parse_args()
    report = {"network": "one internal Docker bridge, direct Roha/noq QUIC, no relay",
              "users": len(NODES), "checks": [], "status": "FAIL"}
    try:
        built = subprocess.run(
            ["docker", "build", "--progress=plain", "-t", "akula-roha-ten-peer:local",
             "-f", "labs/ten-peer/Dockerfile", "."],
            cwd=Path(__file__).resolve().parents[2],
            capture_output=True, text=True, timeout=2400)
        if built.returncode:
            raise RuntimeError("docker build failed:\n" + built.stdout[-12000:] + built.stderr[-12000:])
        run("up", "--no-build", "-d", "--wait", timeout=240)
        keys = {node: result(node, "id") for node in NODES}
        assert len(set(keys.values())) == 10
        report["checks"].append("10 independent identities")

        # No message may be accepted before the identities are explicitly enrolled.
        send("peer01", "peer02", keys, "not-enrolled", "must fail",
             expect_ok=False)
        assert_count("peer02", 0)
        report["checks"].append("untrusted sender receives no durable ACK")

        # Explicit bootstrap simulates trusted in-person invitations.
        for node in NODES:
            for other in NODES:
                if node != other:
                    assert result(node, "allow", keys[other]) in ("ALLOWED", "ALREADY_ALLOWED")
        report["checks"].append("90 explicitly accepted peer identities")

        latencies = []
        for index, sender in enumerate(NODES):
            target = NODES[(index + 1) % 10]
            latencies.append(send(sender, target, keys, f"ring-{index}", "ring test"))
        for sender in NODES:
            for target in NODES:
                if sender != target:
                    latencies.append(send(sender, target, keys,
                                          f"pair-{sender}-{target}", "mesh test"))
        for node in NODES:
            assert_count(node, 10)  # one ring + nine all-to-all
        report["checks"].append("100 recipient-verified direct messages; all 90 ordered pairs")

        send("peer01", "peer02", keys, "pair-peer01-peer02", "mesh test")
        assert_count("peer02", 10)
        send("peer01", "peer02", keys, "pair-peer01-peer02", "tampered duplicate",
             expect_ok=False)
        assert_count("peer02", 10)
        report["checks"].append("identical replay deduplicated; conflicting replay rejected")

        # Roha can either reject a mismatched endpoint certificate or use a
        # previously learned route to the true key owner. Neither outcome may
        # persist a message on the unrelated host peer02.
        observed = exec_node("peer01", "send", "peer02", keys["peer03"],
                             "identity-routing", "only key owner", ok=False)
        reached_owner = observed.returncode == 0
        if reached_owner:
            assert observed.stdout.strip() == "ACK:identity-routing:STORED"
            assert_count("peer03", 11)
            report["identity_route_observation"] = "routed_to_authenticated_owner"
        else:
            assert_count("peer03", 10)
            report["identity_route_observation"] = "mismatched_certificate_rejected"
        assert_count("peer02", 10)
        report["checks"].append("wrong hostname never receives other identity's message")

        # Take the real key owner offline: a different live host MUST NOT
        # impersonate it to acknowledge the message.
        run("stop", "peer03")
        send("peer01", "peer02", keys, "fake-key", "must fail",
             target_key=keys["peer03"], expect_ok=False)
        assert_count("peer02", 10)
        run("start", "peer03")
        wait_healthy("peer03")
        assert result("peer03", "id") == keys["peer03"]
        assert_count("peer03", 11 if reached_owner else 10)
        report["checks"].append("offline key owner cannot be impersonated by other host")

        run("restart", "peer05")
        wait_healthy("peer05")
        assert result("peer05", "id") == keys["peer05"]
        assert_count("peer05", 10)
        send("peer04", "peer05", keys, "after-restart", "persisted key and inbox")
        assert_count("peer05", 11)
        report["checks"].append("restart retains identity and durable inbox")

        run("stop", "peer07")
        send("peer01", "peer07", keys, "offline-01", "no fake success", expect_ok=False)
        run("start", "peer07")
        wait_healthy("peer07")
        assert_count("peer07", 10)
        send("peer01", "peer07", keys, "offline-01", "no fake success")
        assert_count("peer07", 11)
        report["checks"].append("offline delivery has no false ACK; manual retry succeeds")

        report["p50_ms"] = percentile(latencies, 50)
        report["p95_ms"] = percentile(latencies, 95)
        report["verified_deliveries"] = len(latencies) + 2 + int(reached_owner)
        report["status"] = "PASS"
    except Exception as error:
        report["error"] = repr(error)
        logs = run("logs", "--no-color", "--tail", "80", ok=False)
        report["container_logs"] = (logs.stdout + logs.stderr)[-18000:]
        raise
    finally:
        out = Path(opts.output)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n",
                       encoding="utf-8")
        if not opts.keep:
            run("down", "-v", "--remove-orphans", ok=False, timeout=120)
        print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
