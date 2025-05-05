use anyhow::{Context, Result};
use auto_esj::book;
use auto_esj::esj;
use auto_esj::esj::Config;
use auto_esj::utils::get_input;
use clearscreen;
use std::fs;
use thirtyfour::WebDriver;

#[tokio::main]
async fn main() -> Result<()> {
    let (driver, driver_process, mut config) = esj::start_driver().await.context("無法啟動瀏覽器")?;
    let result = execute(&driver, &config).await;
    if let Err(e) = result {
        println!("{}", e);
    }
    config.set_cookies(&driver).await?;
    esj::stop_driver(driver, driver_process).await?;
    Ok(())
}

async fn execute(driver: &WebDriver, config: &Config) -> Result<()> {
    let result: Result<()> = loop {
        println!("1. 發布文章");
        println!("2. 編輯文章");
        println!("3. 目錄整合");
        println!("4. 目錄同步");
        println!("5. 退出");
        let choice = get_input("請選擇操作: ");
        match choice.as_str() {
            "1" => {
                fs::create_dir_all(book::WORK_DIR).expect("無法創建臨時目錄");
                let (mut book, chapters) = book::processor(book::WORK_DIR).await?;

                // --- 修改：呼叫新的調度函數 ---
                let chapter_count = chapters.len(); // 取得章節數量
                let schedules = book::determine_schedules(chapter_count)?; // 調用新的主調度函數
                // --- 结束修改 ---

                esj::execute_publish(&mut book, &chapters, &driver, &config, schedules).await?;
                fs::remove_dir_all(book::WORK_DIR)?;
            },
            "2" => {
                fs::create_dir_all(book::WORK_DIR).expect("無法創建臨時目錄");
                let (book, chapters) = book::processor(book::WORK_DIR).await?;
                // esj::execute_edit(&book, &chapters, &driver, &config).await?;
                esj::execute_edit(&book, &chapters, &driver, &config).await?;
                fs::remove_dir_all(book::WORK_DIR)?;
            },
            "3" => {
                let book = book::Config::build(&get_input("請輸入書籍檔案路徑:"))?;
                esj::merge_directories(&driver, &config, &book).await?;
            },
            "4" => {
                let book_path = get_input("請輸入書籍檔案路徑: ");
                let book = book::Config::build(&book_path)?;
                esj::chapters_sync(&book, &book_path, config, driver).await?;
            }
            "5" => break Err(anyhow::anyhow!("已退出")),
            _ => println!("無效的選項"),
        }
        get_input("按任意鍵繼續");
        clearscreen::clear()?;
    };

    result
}
