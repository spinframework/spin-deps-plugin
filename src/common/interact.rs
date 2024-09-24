use anyhow::Result;

use dialoguer::{MultiSelect, Select};

pub fn select_prompt(
    prompt: &str,
    selection_list: &[String],
    default: Option<usize>,
) -> Result<usize> {
    let mut select = Select::new().with_prompt(prompt).items(selection_list);
    if let Some(index) = default {
        select = select.default(index);
    }
    Ok(select.interact()?)
}

pub fn select_multiple_prompt(prompt: &str, selection_list: &[String]) -> Result<Vec<usize>> {
    Ok(MultiSelect::new()
        .with_prompt(prompt)
        .items(selection_list)
        .interact()?)
}
