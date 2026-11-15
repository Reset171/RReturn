fn hook_guest_die(
    game: &mut Game<'_>,
    host: &mut LanHost,
    gi: usize,
    conn: u32,
) -> std::ops::ControlFlow<()> {
    crate::game::levels::rreturn::intercept_guest_die(game, host, gi, conn)
}