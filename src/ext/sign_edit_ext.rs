use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignEditorSnapshot {
    pub pos: [i32; 3],
    pub front: bool,
    pub line: usize,
    pub lines: [String; 4],
    pub edit_value: String,
    pub edit_cursor: usize,
    pub edit_highlight: usize,
}

pub trait SignEditExt {
    fn rreturn_capture_sign_edit(&self) -> Option<SignEditorSnapshot>;
    fn rreturn_restore_sign_edit(&mut self, snap: Option<&SignEditorSnapshot>);
}

impl SignEditExt for Game<'_> {
    fn rreturn_capture_sign_edit(&self) -> Option<SignEditorSnapshot> {
        let ed = self.sign_edit.as_ref()?;
        Some(SignEditorSnapshot {
            pos: [ed.pos.0, ed.pos.1, ed.pos.2],
            front: ed.front,
            line: ed.line,
            lines: ed.lines.clone(),
            edit_value: ed.edit.edit.value.clone(),
            edit_cursor: ed.edit.edit.cursor,
            edit_highlight: ed.edit.edit.highlight,
        })
    }

    fn rreturn_restore_sign_edit(&mut self, snap: Option<&SignEditorSnapshot>) {
        let Some(snap) = snap else {
            self.sign_edit = None;
            return;
        };
        let mut edit = sign_box();
        edit.set_value(&snap.edit_value);
        let max_c = edit.edit.value.chars().count();
        edit.edit.cursor = snap.edit_cursor.min(max_c);
        edit.edit.highlight = snap.edit_highlight.min(max_c);
        edit.focused = true;
        let editor = SignEditor {
            pos: (snap.pos[0], snap.pos[1], snap.pos[2]),
            front: snap.front,
            line: snap.line.min(3),
            edit,
            lines: snap.lines.clone(),
        };
        self.open_screen_overlay("sign_editor", |game| {
            game.sign_edit = Some(editor);
            game.keys.clear();
            game.right = false;
            game.capture(false);
        });
    }
}
