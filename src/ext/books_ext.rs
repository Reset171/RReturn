use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookScreenSnapshot {
    Edit {
        slot: usize,
        pages: Vec<String>,
        page: usize,
        signing: bool,
        title: String,
        title_value: String,
        title_cursor: usize,
        title_highlight: usize,
        page_value: String,
        page_cursor: usize,
        page_highlight: usize,
    },
    View {
        page: usize,
        lectern: Option<[i32; 3]>,
        plain_pages: Vec<String>,
    },
}

pub trait BooksExt {
    fn rreturn_capture_book(&self) -> Option<BookScreenSnapshot>;
    fn rreturn_restore_book(&mut self, snap: Option<&BookScreenSnapshot>);
    fn rreturn_is_book_editing(&self) -> bool;
}

impl BooksExt for Game<'_> {
    fn rreturn_is_book_editing(&self) -> bool {
        matches!(self.book, Some(BookScreen::Edit { .. }))
    }
    fn rreturn_capture_book(&self) -> Option<BookScreenSnapshot> {
        let book = self.book.as_ref()?;
        Some(match book {
            BookScreen::Edit {
                slot,
                pages,
                page,
                signing,
                title,
                title_edit,
                page_edit,
            } => BookScreenSnapshot::Edit {
                slot: *slot,
                pages: pages.clone(),
                page: *page,
                signing: *signing,
                title: title.clone(),
                title_value: title_edit.edit.value.clone(),
                title_cursor: title_edit.edit.cursor,
                title_highlight: title_edit.edit.highlight,
                page_value: page_edit.edit.value.clone(),
                page_cursor: page_edit.edit.cursor,
                page_highlight: page_edit.edit.highlight,
            },
            BookScreen::View {
                pages,
                page,
                lectern,
            } => BookScreenSnapshot::View {
                page: *page,
                lectern: lectern.map(|p| [p.0, p.1, p.2]),
                plain_pages: pages.iter().map(Text::plain).collect(),
            },
        })
    }

    fn rreturn_restore_book(&mut self, snap: Option<&BookScreenSnapshot>) {
        let Some(snap) = snap else {
            self.book = None;
            return;
        };
        match snap {
            BookScreenSnapshot::Edit {
                slot,
                pages,
                page,
                signing,
                title,
                title_value,
                title_cursor,
                title_highlight,
                page_value,
                page_cursor,
                page_highlight,
            } => {
                let mut title_edit = book_title_box();
                title_edit.set_value(title_value);
                let max_t = title_edit.edit.value.chars().count();
                title_edit.edit.cursor = (*title_cursor).min(max_t);
                title_edit.edit.highlight = (*title_highlight).min(max_t);

                let mut page_edit = book_page_box();
                page_edit.set_value(page_value);
                let max_p = page_edit.edit.value.chars().count();
                page_edit.edit.cursor = (*page_cursor).min(max_p);
                page_edit.edit.highlight = (*page_highlight).min(max_p);

                let pages = if pages.is_empty() {
                    vec![String::new()]
                } else {
                    pages.clone()
                };
                let clamped_page = (*page).min(pages.len().saturating_sub(1));
                let screen = BookScreen::Edit {
                    slot: (*slot).min(45),
                    pages,
                    page: clamped_page,
                    signing: *signing,
                    title: title.clone(),
                    title_edit,
                    page_edit,
                };
                self.open_book(screen);
            }
            BookScreenSnapshot::View {
                page,
                lectern,
                plain_pages,
            } => {
                let pages = if let Some(lpos) = lectern {
                    if let Some(BlockEntity::Lectern(l)) =
                        self.world.block_entities.get(&(lpos[0], lpos[1], lpos[2]))
                    {
                        l.book
                            .as_ref()
                            .map(|b| view_pages(b.data()))
                            .unwrap_or_default()
                    } else {
                        plain_pages.iter().map(Text::untrusted).collect()
                    }
                } else if let Some(stack) = self
                    .held()
                    .filter(|s| &*s.item == "minecraft:written_book")
                    .or_else(|| {
                        self.offhand
                            .as_ref()
                            .filter(|s| &*s.item == "minecraft:written_book")
                    })
                {
                    view_pages(stack.data())
                } else {
                    plain_pages.iter().map(Text::untrusted).collect()
                };
                let pages = if pages.is_empty() {
                    plain_pages.iter().map(Text::untrusted).collect()
                } else {
                    pages
                };
                let clamped_page = (*page).min(pages.len().saturating_sub(1));
                self.open_book(BookScreen::View {
                    pages,
                    page: clamped_page,
                    lectern: lectern.map(|p| (p[0], p[1], p[2])),
                });
            }
        }
    }
}
