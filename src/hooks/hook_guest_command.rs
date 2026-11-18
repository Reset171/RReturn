fn hook_guest_command(
    game: &mut Game<'_>,
    conn: u32,
    command: &str,
) -> std::ops::ControlFlow<()> {
    crate::game::autosave::rreturn::intercept_guest_command(game, conn, command)
}
