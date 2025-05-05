use std::{fs, io};
use std::io::Write;
use std::path::Path;
use anyhow::{Result, Context};
use serde::Serialize;
use serde::de::DeserializeOwned;
use crate::CONFIG_FILE;

pub fn get_executable_name(base_name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{}.exe", base_name)
    } else {
        base_name.to_string()
    }
}

pub fn get_input(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().trim_matches('"').to_string()
}

pub fn save<T>(section_head: &str, section_content: &T) -> Result<()>
where T: Serialize
{
    let config_str = fs::read_to_string(CONFIG_FILE)
        .with_context(|| format!("配置文件{CONFIG_FILE}加载失败"))?;

    let mut config_table: toml::Value = config_str.parse()
        .with_context(|| format!("無法解析配置文件 {CONFIG_FILE}"))?;

    let commit_value = toml::Value::try_from(section_content)?;

    if let Some(table) = config_table.as_table_mut() {
        table.insert(section_head.to_string(), commit_value);
    } else {
        return Err(anyhow::anyhow!("配置文件格式不正確"));
    }

    let new_config_str = toml::to_string(&config_table)?;

    fs::write(CONFIG_FILE, new_config_str)
        .with_context(|| format!("無法寫入配置文件 {CONFIG_FILE}"))?;

    Ok(())
}

pub fn load<T>(section_head: &str) -> Result<T>
where T: DeserializeOwned
{
    let config_str = fs::read_to_string(CONFIG_FILE)
        .with_context(|| format!("配置文件{CONFIG_FILE}加載失败"))?;

    let config_table: toml::Value = config_str.parse()
        .with_context(|| format!("配置文件{CONFIG_FILE}解析失败"))?;

    let section_table = config_table.get(section_head)
        .with_context(|| format!("配置文件{CONFIG_FILE}中没有找到{section_head}"))?;

    section_table.clone().try_into::<T>().with_context(|| format!("{section_head}配置轉換失敗"))
}

pub fn copy_file(src: &Path, dest: &Path) -> Result<()> {
    if !src.exists() {
        return Err(anyhow::anyhow!("文件{:?}不存在", src));
    }
    fs::copy(src, dest)?;
    Ok(())
}