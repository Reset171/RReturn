fn hook_guest_command(
    game: &mut Game<'_>,
    host: &mut LanHost,
    conn: u32,
    command: &str,
) -> std::ops::ControlFlow<()> {
    crate::game::levels::rreturn::intercept_guest_command(game, host, conn, command)
}