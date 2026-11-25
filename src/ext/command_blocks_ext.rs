use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandBlockUiSnapshot {
    pub pos: [i32; 3],
    pub cart: Option<u32>,
    pub command: String,
    pub cursor: usize,
    pub highlight: usize,
    pub display: usize,
    pub track_output: bool,
    pub mode: u8,
    pub conditional: bool,
    pub auto: bool,
}

fn mode_to_u8(mode: Mode) -> u8 {
    match mode {
        Mode::Sequence => 0,
        Mode::Auto => 1,
        Mode::Redstone => 2,
    }
}

fn u8_to_mode(v: u8) -> Mode {
    match v {
        0 => Mode::Sequence,
        1 => Mode::Auto,
        _ => Mode::Redstone,
    }
}

pub trait CommandBlocksExt {
    fn rreturn_capture_command_block_ui(&self) -> Option<CommandBlockUiSnapshot>;
    fn rreturn_restore_command_block_ui(&mut self, snap: Option<&CommandBlockUiSnapshot>);
}

impl CommandBlocksExt for Game<'_> {
    fn rreturn_capture_command_block_ui(&self) -> Option<CommandBlockUiSnapshot> {
        let s = self.command_block_screen.as_ref()?;
        Some(CommandBlockUiSnapshot {
            pos: [s.pos.0, s.pos.1, s.pos.2],
            cart: s.cart,
            command: s.command.input.value.clone(),
            cursor: s.command.input.cursor,
            highlight: s.command.input.highlight,
            display: s.command.input.display,
            track_output: s.track_output,
            mode: mode_to_u8(s.mode),
            conditional: s.conditional,
            auto: s.auto,
        })
    }

    fn rreturn_restore_command_block_ui(&mut self, snap: Option<&CommandBlockUiSnapshot>) {
        let Some(snap) = snap else {
            self.command_block_screen = None;
            return;
        };
        let pos = (snap.pos[0], snap.pos[1], snap.pos[2]);
        let mode = u8_to_mode(snap.mode);
        let block = self
            .command_block(pos)
            .cloned()
            .unwrap_or(CommandBlock {
                command: snap.command.clone(),
                success_count: 0,
                custom_name: None,
                track_output: snap.track_output,
                last_output: None,
                update_last_execution: true,
                last_execution: -1,
                powered: false,
                auto: snap.auto,
                condition_met: false,
            });
        let last_output = self.command_block(pos).and_then(|c| c.last_output.clone());
        self.open_edit_screen(pos, snap.cart, block, mode, snap.conditional);
        if let Some(screen) = self.command_block_screen.as_mut() {
            let max_c = snap.command.chars().count();
            screen.command.input.value = snap.command.clone();
            screen.command.input.cursor = snap.cursor.min(max_c);
            screen.command.input.highlight = snap.highlight.min(max_c);
            screen.command.input.display = snap.display.min(max_c);
            screen.command.input.focused = true;
            screen.track_output = snap.track_output;
            screen.mode = mode;
            screen.conditional = snap.conditional;
            screen.auto = snap.auto;
            screen.show_previous(&CommandBlock {
                command: snap.command.clone(),
                success_count: 0,
                custom_name: None,
                track_output: snap.track_output,
                last_output,
                update_last_execution: true,
                last_execution: -1,
                powered: false,
                auto: snap.auto,
                condition_met: false,
            });
        }
    }
}
