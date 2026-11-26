fn hook_remote_open_screen(
    game: &mut Game<'_>,
    p: &horizon_protocol::packets::play::OpenScreen,
) {
    use crate::game::remote::rreturn_remote::GameRemoteContainerExt;
    game.rreturn_on_remote_open_screen_pre(p);
}
