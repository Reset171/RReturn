#!/usr/bin/env python3
"""Detailed performance profiling and trace script for RReturn scrubbing and timeline."""

import json
import time
from rreturn_harness import RReturnHarness

def main():
    h = RReturnHarness()
    print("[Bench] Connecting harness...")
    h.connect(auto_launch=True)
    
    # Clean setup
    h.raw_send("screen hud")
    h.command("gamerule doDaylightCycle false")
    h.command("gamerule doMobSpawning false")
    h.command("time set noon")
    h.command("tp @s 0.5 65.0 0.5")
    h.wait(10)
    
    # Reset perf counters
    print("[Bench] Resetting perf counters...")
    print(h.rreturn("perf reset"))
    
    # Spawn 40 mobs of various types to create realistic survival load
    print("[Bench] Spawning 40 entities...")
    for i in range(10):
        h.command(f"summon minecraft:zombie {i * 1.5 - 7.0} 65.0 3.0 {{CustomName:'\"Z_{i}\"'}}")
        h.command(f"summon minecraft:skeleton {i * 1.5 - 7.0} 65.0 -3.0 {{CustomName:'\"S_{i}\"'}}")
        h.command(f"summon minecraft:cow {3.0} 65.0 {i * 1.5 - 7.0}")
        h.command(f"summon minecraft:sheep {-3.0} 65.0 {i * 1.5 - 7.0}")
    
    # Spawn some particles and block entities
    for i in range(5):
        h.command(f"setblock {i} 65 5 minecraft:chest")
        h.command(f"particle minecraft:flame {i} 66 5 0.1 0.1 0.1 0.05 20")
    
    print("[Bench] Running 100 live ticks...")
    h.wait(100)
    
    print("[Bench] Starting scrub mode (key U equivalent)...")
    res_start = h.rreturn("scrub start")
    print(f"[Bench] Scrub start: {res_start}")
    
    # Scrub backwards by 40 ticks
    print("[Bench] Scrubbing backwards across frames...")
    for step in range(1, 5):
        h.rreturn(f"scrub rewind_ticks {step * 10}")
        h.wait(5)
    
    # Commit scrub
    print("[Bench] Committing scrub...")
    res_commit = h.rreturn("scrub commit")
    print(f"[Bench] Scrub commit: {res_commit}")
    h.wait(10)
    
    # Retrieve performance report
    print("\n" + "=" * 60)
    perf_report = h.rreturn("perf")
    print(perf_report)
    print("=" * 60 + "\n")
    
    perf_json = h.rreturn("perf json")
    try:
        data = json.loads(perf_json) if isinstance(perf_json, str) else perf_json
        print("[Bench] JSON Summary:")
        print(json.dumps(data, indent=2))
    except Exception as e:
        print(f"[Bench] Error parsing JSON: {e}\nRaw: {perf_json}")

    # Clean up
    h.command("kill @e[type=!player]")
    h.cleanup_worlds(keep_worlds=False)
    h.quit()

if __name__ == "__main__":
    main()
