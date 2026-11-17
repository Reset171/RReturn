"""Tests 24-27: Portals, Dimension Travel, Ender Pearls, Portal Cooldown."""

from . import setup_clean_env


def test_24_nether_portal_cold_chunk(h):
    setup_clean_env(h)
    h.tp(1200.5, 65, 1200.5)
    h.command("setblock 1200 65 1202 minecraft:obsidian")
    h.command("setblock 1200 66 1202 minecraft:obsidian")
    h.command("setblock 1200 67 1202 minecraft:obsidian")
    h.wait(3)

    h.rreturn("set")
    h.wait(2)

    h.command("setblock 1200 66 1202 minecraft:nether_portal[axis=z]")
    h.wait(2)
    assert "nether_portal" in h.block(1200, 66, 1202).get("block", ""), "Portal block was not placed"

    h.tp(1800.5, 65, 1800.5)
    h.wait(4)

    h.rreturn("load")
    h.wait(6)

    h.tp(1200.5, 65, 1200.5)
    b = h.block(1200, 66, 1202)
    assert b.get("block") == "minecraft:obsidian", f"Portal block leaked in cold chunk: {b}"
    h.command("fill 1200 65 1202 1200 67 1202 minecraft:air")
    h.tp(0.5, 65, 0.5)


def test_25_flying_ender_pearl_discard(h):
    setup_clean_env(h)
    base_pos = h.state()["player"]["position"]

    h.rreturn("set")
    h.wait(2)

    h.command("summon minecraft:ender_pearl 0.5 70 0.5 {Motion:[0.0, 1.0, 50.0],Tags:['future_pearl']}")
    h.wait(2)
    assert h.has_entity("@e[tag=future_pearl]"), "Ender pearl was not spawned"

    h.rreturn("load")
    h.wait(12)

    assert not h.has_entity("@e[tag=future_pearl]"), "Flying ender pearl remained after rollback"
    pos_after = h.state()["player"]["position"]
    assert abs(pos_after[0] - base_pos[0]) < 2.0 and abs(pos_after[2] - base_pos[2]) < 2.0, (
        f"Flying ender pearl teleported player after rewind: {pos_after}"
    )


def test_26_portal_processor_cooldown(h):
    setup_clean_env(h)
    h.tp(1300.5, 65, 1300.5)
    h.command("fill 1299 64 1299 1301 64 1301 minecraft:stone")
    h.command("setblock 1300 65 1300 minecraft:nether_portal[axis=x]")
    h.wait(3)

    h.rreturn("set")
    h.wait(2)

    h.command("tp @s 1300.5 65.0 1300.5")
    h.wait(4)

    h.rreturn("load")
    h.wait(6)

    dim = h.state().get("dimension", "")
    assert "overworld" in dim, f"Player was unexpectedly sucked into portal after rewind: {dim}"
    h.command("setblock 1300 65 1300 minecraft:air")


def test_27_end_credits_respawn_safety(h):
    setup_clean_env(h)
    h.rreturn("set")
    h.wait(2)

    h.command("advancement grant @s only minecraft:end/root")
    h.wait(3)

    h.rreturn("load")
    h.wait(4)

    st = h.state()
    assert st.get("screen") == "playing", f"Screen was broken after credits check: {st.get('screen')}"


def test_46_nether_portal_creative_rollback_and_reenter(h):
    setup_clean_env(h)
    h.command("gamemode creative")
    h.wait(2)

    # Стоим в безопасном месте в Оверворлде
    h.tp(1380.5, 65.0, 1380.5)
    h.wait(2)
    assert "overworld" in h.state().get("dimension", ""), f"Player not in overworld before building portal: {h.state().get('dimension')}"

    # Строим портал на отдалении
    h.command("fill 1399 64 1399 1401 64 1401 minecraft:obsidian")
    h.command("fill 1399 65 1400 1399 68 1400 minecraft:obsidian")
    h.command("fill 1401 65 1400 1401 68 1400 minecraft:obsidian")
    h.command("fill 1399 68 1400 1401 68 1400 minecraft:obsidian")
    h.command("setblock 1400 65 1400 minecraft:nether_portal[axis=x]")
    h.wait(3)

    # Стоим перед порталом (но НЕ внутри)
    h.tp(1400.5, 65.0, 1396.0)
    h.wait(2)
    assert "overworld" in h.state().get("dimension", ""), f"Player not in overworld before checkpoint: {h.state().get('dimension')}"

    # Делаем чекпоинт в Оверворлде в creative
    h.rreturn("set")
    h.wait(2)

    # Идем в портал в Незер
    h.tp(1400.5, 65.0, 1400.0)
    in_nether = False
    for _ in range(40):
        h.wait(2)
        dim = h.state().get("dimension", "")
        if "the_nether" in dim or "nether" in dim:
            in_nether = True
            break
    assert in_nether, f"Player did not travel to the Nether: dimension is {h.state().get('dimension')}"

    h.rreturn("load")
    for _ in range(10):
        h.wait(2)
        if "overworld" in h.state().get("dimension", ""):
            break

    st_after = h.state()
    assert "overworld" in st_after.get("dimension", ""), f"Dimension not overworld after rollback: {st_after.get('dimension')}"
    assert st_after.get("screen") == "playing", f"Screen stuck on loading after rollback: {st_after.get('screen')}"

    h.tp(1400.5, 65.0, 1400.0)

    reentered_nether = False
    for _ in range(40):
        h.wait(2)
        st_loop = h.state()
        dim = st_loop.get("dimension", "")
        if "the_nether" in dim or "nether" in dim:
            reentered_nether = True
            break
    assert reentered_nether, f"Player failed to re-enter Nether after rollback: dim={h.state().get('dimension')}, screen={h.state().get('screen')}"
    assert h.state().get("screen") == "playing", f"Screen stuck after re-entering Nether: {h.state().get('screen')}"

    h.tp(0.5, 65.0, 0.5)
