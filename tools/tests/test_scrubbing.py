"""Test 83: Interactive time scrubbing and wheel rewind."""

import json
from . import setup_clean_env


def _parse_res(res):
    if isinstance(res, dict):
        return res
    if isinstance(res, str):
        try:
            return json.loads(res)
        except Exception:
            start = res.find("{")
            if start != -1:
                return json.loads(res[start:])
    return {}


def test_83_interactive_time_scrubbing_and_rewind(h):
    setup_clean_env(h)
    h.tp(0.5, 65, 0.5)
    h.wait(10)

    # Verify history recording is tracking ticks
    st = _parse_res(h.rreturn("scrub status"))
    assert not st.get("active", False), f"Scrub should be inactive initially: {st}"

    # Set a base checkpoint
    h.rreturn("set")
    h.wait(6)

    # Spawn a zombie and change diamond block
    h.command("summon minecraft:zombie 3.5 65.0 3.5 {Tags:['scrub_zombie'],CustomName:'\"ScrubZ\"'}")
    h.command("setblock 0 65 2 minecraft:diamond_block")
    h.wait(10)
    assert h.has_entity("@e[tag=scrub_zombie]"), "Zombie did not spawn"
    assert "diamond_block" in h.block(0, 65, 2).get("block", ""), "Diamond block not set"

    # Move zombie to distant location and break diamond block
    h.command("tp @e[tag=scrub_zombie] 15.5 65.0 15.5")
    h.command("setblock 0 65 2 minecraft:air")
    h.wait(12)
    assert "air" in h.block(0, 65, 2).get("block", ""), "Diamond block was not removed"
    far_pos = h.data_get_entity("@e[tag=scrub_zombie,limit=1]", "Pos")
    assert "15." in far_pos, f"Zombie did not move to far position: {far_pos}"

    # Start interactive scrub mode
    res = h.rreturn("scrub start")
    res_str = str(res)
    assert "started" in res_str.lower(), f"Unexpected scrub start response: {res}"

    st = _parse_res(h.rreturn("scrub status"))
    assert st.get("active", False), f"Scrub should be active after start: {st}"
    anchor_t = st.get("anchor_tick", 0)

    # Step wheel backward past the break: diamond block should reappear in real time!
    h.rreturn("scrub step -12.0")
    st = _parse_res(h.rreturn("scrub status"))
    assert st.get("current_time", anchor_t) <= anchor_t, f"Cursor did not move back: {st}"
    assert "diamond_block" in h.block(0, 65, 2).get("block", ""), "Diamond block did not reappear in real-time scrubbing!"
    assert "particles" in st, f"Particles field expected in scrub status: {st}"

    # Step forward: diamond block should disappear again in real time!
    h.rreturn("scrub step 12.0")
    assert "air" in h.block(0, 65, 2).get("block", ""), "Diamond block did not disappear on forward scrubbing!"

    # Step backward again and commit
    h.rreturn("scrub step -15.0")
    res_commit = str(h.rreturn("scrub commit"))
    assert "committed" in res_commit.lower(), f"Unexpected scrub commit response: {res_commit}"

    st = _parse_res(h.rreturn("scrub status"))
    assert not st.get("active", True), f"Scrub should be inactive after commit: {st}"

    # Verify that the zombie remained at the rewound position and did NOT teleport back to 15.5!
    h.wait(10)
    rewound_pos = h.data_get_entity("@e[tag=scrub_zombie,limit=1]", "Pos")
    assert "15." not in rewound_pos, f"Zombie snapped back to future position after commit: {rewound_pos}"

    # Test cancel workflow
    h.rreturn("scrub start")
    h.rreturn("scrub step -10.0")
    h.rreturn("scrub cancel")
    st = _parse_res(h.rreturn("scrub status"))
    assert not st.get("active", True), f"Scrub should be inactive after cancel: {st}"

    # Clean up
    h.command("kill @e[tag=scrub_zombie]")
    h.command("setblock 0 65 2 minecraft:air")
    h.wait(4)
