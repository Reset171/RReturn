use super::*;
use horizon_client::recipe_book::Category;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RecipeUiSnapshot {
    pub tab: Option<String>,
    pub page: usize,
    pub search_value: String,
    pub search_cursor: usize,
    pub search_highlight: usize,
    pub search_focused: bool,
}

fn tab_to_str(tab: Tab) -> &'static str {
    match tab {
        Tab::Search => "search",
        Tab::Category(c) => match c {
            Category::CraftingBuilding => "crafting_building",
            Category::CraftingRedstone => "crafting_redstone",
            Category::CraftingEquipment => "crafting_equipment",
            Category::CraftingMisc => "crafting_misc",
            Category::FurnaceFood => "furnace_food",
            Category::FurnaceBlocks => "furnace_blocks",
            Category::FurnaceMisc => "furnace_misc",
            Category::BlastBlocks => "blast_blocks",
            Category::BlastMisc => "blast_misc",
            Category::SmokerFood => "smoker_food",
        },
    }
}

fn str_to_tab(s: &str) -> Option<Tab> {
    Some(match s {
        "search" => Tab::Search,
        "crafting_building" => Tab::Category(Category::CraftingBuilding),
        "crafting_redstone" => Tab::Category(Category::CraftingRedstone),
        "crafting_equipment" => Tab::Category(Category::CraftingEquipment),
        "crafting_misc" => Tab::Category(Category::CraftingMisc),
        "furnace_food" => Tab::Category(Category::FurnaceFood),
        "furnace_blocks" => Tab::Category(Category::FurnaceBlocks),
        "furnace_misc" => Tab::Category(Category::FurnaceMisc),
        "blast_blocks" => Tab::Category(Category::BlastBlocks),
        "blast_misc" => Tab::Category(Category::BlastMisc),
        "smoker_food" => Tab::Category(Category::SmokerFood),
        _ => return None,
    })
}

pub trait RecipeBookExt {
    fn rreturn_capture_recipe_ui(&self) -> RecipeUiSnapshot;
    fn rreturn_restore_recipe_ui(&mut self, snap: &RecipeUiSnapshot);
}

impl RecipeBookExt for Game<'_> {
    fn rreturn_capture_recipe_ui(&self) -> RecipeUiSnapshot {
        let search = self.recipe_ui.search.as_ref();
        RecipeUiSnapshot {
            tab: self.recipe_ui.tab.map(|t| tab_to_str(t).to_owned()),
            page: self.recipe_ui.page,
            search_value: search.map(|e| e.edit.value.clone()).unwrap_or_default(),
            search_cursor: search.map(|e| e.edit.cursor).unwrap_or(0),
            search_highlight: search.map(|e| e.edit.highlight).unwrap_or(0),
            search_focused: search.is_some_and(|e| e.focused),
        }
    }

    fn rreturn_restore_recipe_ui(&mut self, snap: &RecipeUiSnapshot) {
        self.recipe_ui.tab = snap.tab.as_deref().and_then(str_to_tab);
        self.recipe_ui.page = snap.page;
        if self.recipe_ui.search.is_none()
            && (snap.search_focused || !snap.search_value.is_empty())
        {
            self.recipe_ui.search = Some(EditBox::new(0.0, 0.0, 80.0, 14.0));
        }
        if let Some(search) = self.recipe_ui.search.as_mut() {
            search.set_value(&snap.search_value);
            let max_c = search.edit.value.chars().count();
            search.edit.cursor = snap.search_cursor.min(max_c);
            search.edit.highlight = snap.search_highlight.min(max_c);
            search.focused = snap.search_focused;
        }
        self.recipe_ui.view_key = 0;
    }
}
