use crate::book::Chapter;
use crate::utils::{copy_file, load};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

const REPO_DIR: &str = "commit";
#[derive(Serialize, Deserialize)]
struct Config {
    username: String,
    repository: String,
    password: Option<String>,
}
impl Config {
    fn build() -> Result<Self> {
        load("commit")
    }
}

fn initialize_git_repo(config: &Config) -> Result<()> {
    if !PathBuf::from(REPO_DIR).join(".git").exists() {
        execute_git_command(&["init"])?;
        let remote_url = format!(
            "git@github.com:{}/{}.git",
            &config.username, &config.repository
        );
        execute_git_command(&["remote", "add",
                                "origin", &remote_url])?;

    }
    Ok(())
}

fn create_css(woff_url: &str, css_path: &Path, font_name: &usize) -> Result<()> {
    let css_content = format!(
        "@font-face {{\n\
        font-family: '{}';\n\
        src: url('{}') format('woff');\n\
        font-weight: normal;\n\
        font-style: normal;\n\
        font-display: swap;\n\
        }}", font_name, woff_url);

    fs::write(css_path, css_content)?;
    Ok(())
}

fn git_commit(files: &Vec<String>) -> Result<String> {
    let mut args = vec!["add"];

    for file in files {
        args.push(file);
    }

    execute_git_command(&args)?;
    execute_git_command(&["commit", "-m", "Add font and CSS file"])?;
    get_commit_hash()
}

pub fn git_push() -> Result<()> {
    execute_git_command(&["push", "origin", "HEAD"])?;
    Ok(())
}

fn execute_git_command(args: &[&str]) -> Result<ExitStatus> {
    Command::new("git")
        .current_dir(REPO_DIR)
        .args(args)
        .status()
        .with_context(|| format!("執行 git 命令失敗: git {:?}", args))
}

fn get_commit_hash() -> Result<String> {
    let output = Command::new("git")
        .current_dir(REPO_DIR)
        .args(&["log", "-n", "1", "--pretty=format:%H"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("獲取提交哈希失敗")?;

    if output.stdout.is_empty() {
        return Err(anyhow::anyhow!("獲取提交哈希失敗").into());
    }

    let commit_hash = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(commit_hash)
}

pub async fn commit(book: &str, chapters: &mut Vec<Chapter>) -> Result<()> {
    let config = Config::build()?;
    let book_dir = PathBuf::from(REPO_DIR).join(book);
    fs::create_dir_all(&book_dir)?;

    let (css_paths, woff_commits, css_commits): (Vec<_>, Vec<_>, Vec<_>) = chapters
        .iter_mut()
        .map(|chapter| {
            let index = chapter.index;
            let font_path = chapter.font_path.as_deref()
                .ok_or_else(|| anyhow!("Missing font path for chapter {}", index))?;

            let woff_path = book_dir.join(format!("{}.woff", index));
            copy_file(font_path.as_ref(), &woff_path)?;

            let css_path = book_dir.join(format!("{}.css", index));
            Ok((css_path, format!("{}/{}.woff", book, index), format!("{}/{}.css", book, index)))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .fold((Vec::new(), Vec::new(), Vec::new()), |mut acc, item| {
            acc.0.push(item.0);
            acc.1.push(item.1);
            acc.2.push(item.2);
            acc
        });

    initialize_git_repo(&config)?;

    let woff_hash = git_commit(&woff_commits)?;
    let cdn_woff = |path: &str| format!(
        "https://cdn.jsdelivr.net/gh/{}/{}@{}/{}",
        config.username, config.repository, woff_hash, path
    );

    woff_commits.iter()
        .zip(css_paths.iter())
        .zip(chapters.iter().map(|c| c.index))
        .try_for_each(|((woff_path, css_path), index)| {
            create_css(&cdn_woff(woff_path), css_path, &index)
        })?;

    let css_hash = git_commit(&css_commits)?;
    let css_base = format!(
        "https://cdn.jsdelivr.net/gh/{}/{}@{}",
        config.username, config.repository, css_hash
    );

    chapters.iter_mut().for_each(|chapter| {
        chapter.set_css_url(&format!("{}/{}/{}.min.css", css_base, book, chapter.index));
    });

    fs::remove_dir_all(book_dir)?;
    Ok(())
}