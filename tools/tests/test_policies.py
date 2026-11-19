"""Test 12: Selective rollback policies (keep inventory while rewinding world)."""

from . import setup_clean_env


def test_12_policy_selective_inventory(h):
    setup_clean_env(h)
    h.tp(0.5, 65, 0.5)
    h.wait(8)

    h.rreturn("policy set player_inventory false")
    h.rreturn("set")
    h.wait(4)

    h.command("give @s minecraft:diamond 16")
    h.wait(4)
    inv_before = h.inventory().get("counts", {})
    assert inv_before.get("minecraft:diamond") == 16, f"Diamonds not given: {inv_before}"

    h.tp(30.5, 65, 30.5)
    h.wait(6)
    h.rreturn("trigger rewind")
    h.wait(8)

    pos_after = h.state()["player"]["position"]
    assert abs(pos_after[0] - 0.5) < 1.5, f"Position did not rewind: {pos_after}"

    inv_after = h.inventory().get("counts", {})
    assert inv_after.get("minecraft:diamond") == 16, f"Selective inventory policy failed: {inv_after}"

    h.rreturn("policy set entities_presence false")
    h.rreturn("set")
    h.wait(2)
    h.command("summon minecraft:trident 0.5 66.0 2.5 {Tags:['policy_trident'],NoGravity:1b}")
    h.wait(2)
    assert h.has_entity("@e[tag=policy_trident]"), "Trident was not spawned"
    h.rreturn("load")
    for _ in range(8):
        if h.has_entity("@e[tag=policy_trident]"):
            break
        h.wait(2)
    assert h.has_entity("@e[tag=policy_trident]"), "Entity-preserving rollback deleted live trident"
    h.command("kill @e[tag=policy_trident]")

    h.rreturn("policy reset")
    h.command("clear @s")
    h.wait(4)