use std::collections::BTreeMap;

use config3::{ConfigResult, ConfigurationFile, FileHander};
use serde::{Deserialize, Serialize};

pub mod config;
pub mod config2;
pub mod config3;

#[derive(Debug, Serialize, Deserialize, Default)]
struct ShoppingList {
    items: BTreeMap<String, usize>,
    price: f64,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let file = FileHander::new("./test.txt")?;
    let config = ConfigurationFile::<ShoppingList>::new(&file)?;
    let tx = config.tx();

    loop {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();

        tx.send_modify(|res| {
            *res.get_mut()
                .items
                .entry(line.trim().to_string())
                .or_insert(0) += 1;
        });
    }
}
