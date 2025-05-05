mod commit;

use crate::esj::ScheduleInfo;
use crate::utils::{get_executable_name, get_input};
use anyhow::{anyhow, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::book::commit::commit;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike, TimeZone, Utc};
use chrono_tz::Asia::Taipei;

pub const EXECUTABLE_FILE: &str = "encryptor/encryptor";
pub const CONFIG_FILE: &str = "book.json";

pub const CHAPTER_DIR: &str = "chapters";
pub const WORK_DIR: &str = "temp";
type ConfigMap = HashMap<String, Config>;
#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    book_id: String,
    forum_id: String,
    encrypt: bool,
    chapter_separator: String,
    chapter_list: PathBuf,
}

#[derive(Clone)]
pub struct Chapter {
    index: usize,
    title: String,
    target_path: String,
    font_path: Option<String>,
    css_url: Option<String>,
}

// --- 新增：時間間隔結構體 ---
#[derive(Debug, Clone, Copy)] // 讓它可以被複製
struct ScheduleInterval {
    days: i64,
    hours: i64,
    minutes: i64,
}

impl ScheduleInterval {
    // 將間隔轉換為 chrono::Duration
    fn to_duration(self) -> Result<Duration> {
        // 基本驗證確保值在合理範圍內，雖然 chrono::Duration 內部會處理溢出
        let total_minutes = self.days * 24 * 60 + self.hours * 60 + self.minutes;
        if total_minutes < 0 {
             // 理論上不應發生，因為輸入已驗證為非負
             return Err(anyhow!("計算出的總間隔為負"));
        }
         // 使用 checked_add 防止 Duration 創建時溢出 (對於合理輸入不太可能)
         Duration::days(self.days)
             .checked_add(&Duration::hours(self.hours))
             .and_then(|d| d.checked_add(&Duration::minutes(self.minutes)))
             .ok_or_else(|| anyhow!("計算時間間隔 Duration 時發生溢出"))
    }
}
// --- 結束新增 ---

impl Config {
    pub fn book_id(&self) -> &str {
        &self.book_id
    }

    pub fn forum_id(&self) -> &str {
        &self.forum_id
    }

    pub fn chapter_list(&self) -> &Path {
        &self.chapter_list
    }
    pub fn chapters_sync(
        &self,
        hashmap: HashMap<String, String>,
        book_path: &str,
    ) -> Result<()> {
        println!("開始同步...");
        let file = File::open(book_path)?;
        let reader = BufReader::new(file);
        let mut chapters = Vec::new();
        let mut current_chapter = Vec::new();
        let trimmed_delimiter = self.chapter_separator.trim();

        for line in reader.lines() {
            let line = line?;
            if line.trim() == trimmed_delimiter {
                if !current_chapter.is_empty() {
                    chapters.push(current_chapter);
                    current_chapter = Vec::new();
                }
            } else {
                current_chapter.push(line);
            }
        }
        if !current_chapter.is_empty() {
            chapters.push(current_chapter);
        }

        let mut title_to_index = HashMap::new();
        for (idx, chapter) in chapters.iter().enumerate() {
            if let Some(title_line) = chapter.iter().find(|line| !line.trim().is_empty()) {
                let title = title_line.trim().to_string();
                title_to_index.insert(title, idx + 1);
            }
        }

        let mut results: Vec<_> = hashmap
            .into_iter()
            .filter_map(|(orig_title, _)| {
                let trimmed_title = orig_title.trim().to_string();
                title_to_index
                    .get(&trimmed_title)
                    .map(|&index| (index, orig_title))
            })
            .collect();
        results.sort_by_key(|&(index, _)| index);

        let file = File::create(&self.chapter_list)?;
        let mut writer = BufWriter::new(file);
        for (index, title) in results {
            writeln!(writer, "{},{}", index, title)?;
        }
        println!("同步成功");
        Ok(())
    }

    pub fn append_chapter(path: &Path, index: usize, title: &str) -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .open(path)?;

        writeln!(file, "{},{}", index, title)?;

        Ok(())
    }

    pub fn load_chapter_list(&self) -> Result<Vec<(usize, String)>> {
        let file = File::open(self.chapter_list())?;
        let reader = BufReader::new(file);
        let mut vec = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if let Some((index_str, title)) = line.split_once(',') {
                if let Ok(index) = index_str.trim().parse::<usize>() {
                    vec.push((index, title.trim().to_string()));
                } else {
                    eprintln!("無法解析：{}", index_str);
                }
            }
        }
        Ok(vec)
    }


    pub fn build(book_path: &str) -> Result<Self> {
        let book_name = Path::new(&book_path)
            .file_name()
            .ok_or(anyhow::anyhow!("無效的文件路徑"))?
            .to_string_lossy()
            .to_string();

        let mut config = Self::get_all()?;

        if config.contains_key(&book_name) {
            return Ok(config[&book_name].clone());
        }

        let book_id = get_input("請輸入書籍編號: ");
        let forum_id = get_input("請輸入論壇編號: ");
        let encrypt = get_input("是否加密書籍(y/n): ");
        let encrypt = if encrypt.to_lowercase() == "y" {
            true
        } else {
            false
        };
        let chapter_separator = get_input("請輸入章節分隔符: ");
        let chapter_list = Path::new(CHAPTER_DIR).join(&book_id);
        fs::create_dir_all(CHAPTER_DIR)
            .context("無法創建目錄")?;
        File::create(&chapter_list)
            .context("無法創建文件")?;
        let book_config = Config {
            book_id,
            forum_id,
            encrypt,
            chapter_separator,
            chapter_list,
        };
        config.insert(book_name, book_config.clone());
        Self::save(&config)?;
        Ok(book_config)
    }
    fn get_all() -> Result<ConfigMap> {
        if Path::new(CONFIG_FILE).exists() {
            let content = fs::read_to_string(CONFIG_FILE)
                .with_context(|| format!("讀取配置文件 {} 失敗", CONFIG_FILE))?;
            serde_json::from_str(&content)
                .context("序列化失敗")

        } else {
            Ok(HashMap::new())
        }
    }

    fn save(config: &ConfigMap) -> Result<()> {
        let content = serde_json::to_string_pretty(config).context("序列化失敗")?;
        fs::write(CONFIG_FILE, content).context("寫入配置文件失敗")?;
        Ok(())
    }
}

impl Chapter {
    pub fn set_css_url(&mut self, css_url: &str) {
        self.css_url = Some(css_url.to_string());
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn target_path(&self) -> &str {
        &self.target_path
    }

    pub fn css_url(&self) -> Option<&str> {
        self.css_url.as_deref()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn encrypt(&self) -> bool {
        self.css_url.is_some()
    }

    pub async fn collect_css_url(book: &str, chapters: &mut Vec<Chapter>) -> Result<()> {
        commit(book, chapters).await
    }
}


// --- 新增：獲取並驗證單個初始時間的函數 ---
fn prompt_for_initial_datetime() -> Result<chrono::DateTime<chrono_tz::Tz>> {
    let mmdd_re = Regex::new(r"^(0[1-9]|1[0-2])(0[1-9]|[12]\d|3[01])$").unwrap();
    let hour_re = Regex::new(r"^(0\d|1\d|2[0-3])$").unwrap();

    let taipei_now = Utc::now().with_timezone(&Taipei);
    let current_year = taipei_now.year();
    let taipei_now_date = taipei_now.date_naive();
    println!("將使用當前年份: {}", current_year);
    println!("当前灣灣の台北時間: {}", taipei_now.format("%Y-%m-%d %H:%M:%S %Z"));

    let naive_date: NaiveDate;
    // MMDD 輸入與驗證循環
    loop {
        let input = get_input("請輸入初始預約月份和日期 (格式 MMDD): ");
        if let Some(caps) = mmdd_re.captures(&input) {
            let month = caps.get(1).unwrap().as_str();
            let day = caps.get(2).unwrap().as_str();
            let current_date_str = format!("{}-{}-{}", current_year, month, day);

            if let Ok(parsed_date) = NaiveDate::parse_from_str(&current_date_str, "%Y-%m-%d") {
                if parsed_date < taipei_now_date {
                     println!("錯誤：日期 ({}) 不能在當前台北日期 ({}) 之前。",
                              parsed_date.format("%Y-%m-%d"),
                              taipei_now_date.format("%Y-%m-%d"));
                    continue;
                }
                naive_date = parsed_date;
                break;
            } else {
                println!("日期無效 (例如，月份 '{}' 沒有第 '{}' 天)，請重新輸入。", month, day);
            }
        } else {
            println!("格式錯誤，請輸入 MMDD 格式。");
        }
    }

    let mut hour_num: u32;
    // 小時 輸入與驗證循環
    loop {
        let input = get_input("請輸入初始預約小時 (00-23): ");
         if hour_re.is_match(&input) {
            hour_num = input.parse::<u32>().unwrap();
            if naive_date == taipei_now_date && hour_num < taipei_now.hour() {
                println!("錯誤：小時 ({:02}) 不能在當前台北小時 ({:02}) 之前。",
                         hour_num, taipei_now.hour());
                continue;
            }
            break;
        } else {
            println!("小時輸入無效。");
        }
    }

    // 分鐘 輸入與驗證循環
    loop {
        let input = get_input("請輸入初始預約分鐘 (00 或 30): ");
        if input == "00" || input == "30" {
            let minute_num: u32 = input.parse().unwrap();
            if let Some(naive_time) = NaiveTime::from_hms_opt(hour_num, minute_num, 0) {
                let scheduled_naive_dt = naive_date.and_time(naive_time);
                match Taipei.from_local_datetime(&scheduled_naive_dt).single() {
                    Some(scheduled_dt_taipei) => {
                        let taipei_now_for_minute_check = Utc::now().with_timezone(&Taipei);
                        if scheduled_dt_taipei <= taipei_now_for_minute_check {
                            println!("錯誤：初始預約時間 ({}) 不能等於或早於當前台北時間 ({})。",
                                     scheduled_dt_taipei.format("%Y-%m-%d %H:%M %Z"),
                                     taipei_now_for_minute_check.format("%Y-%m-%d %H:%M %Z"));
                            continue; // 重新輸入分鐘
                        }
                        // 時間有效，返回 DateTime<Taipei>
                        println!("初始預約時間設定為 (灣灣の台北時間): {}", scheduled_dt_taipei.format("%Y-%m-%d %H:%M %Z"));
                        return Ok(scheduled_dt_taipei); // <--- 返回計算好的初始時間點
                    }
                    None => {
                         println!("錯誤：無法將輸入時間明確轉換為台北時區時間。");
                         continue;
                    }
                }
            } else {
                 println!("內部錯誤：無法創建有效時間。");
                 continue;
            }
        } else {
            println!("分鐘輸入無效，請輸入 00 或 30。");
        }
    }
}
// --- 結束修改 ---

// --- 新增：獲取時間間隔的函數 ---
fn prompt_for_interval() -> Result<ScheduleInterval> {
    println!("請輸入各章節之間的時間間隔:");
    let days = loop {
        let input = get_input("  - 天數 (≥ 0 的正整數): ");
        match input.parse::<i64>() {
            Ok(d) if d >= 0 => break d,
            _ => println!("無效輸入，請輸入≥ 0 的正整數。"),
        }
    };
    let hours = loop {
        let input = get_input("  - 小時數 (0-23): "); // 小時應在 0-23 範圍內
        match input.parse::<i64>() {
            Ok(h) if h >= 0 && h <= 23 => break h,
            _ => println!("無效輸入，請輸入 0 到 23 之間的整數。"),
        }
    };
     let minutes = loop {
        let input = get_input("  - 分鐘數 (0 或 30): ");
        match input.parse::<i64>() {
             Ok(m) if m == 0 || m == 30 => break m,
             _ => println!("無效輸入，請輸入 0 或 30。"),
        }
    };

    let interval = ScheduleInterval { days, hours, minutes };

    println!("時間間隔設定為: {} 天 {} 小時 {} 分鐘", days, hours, minutes);
    Ok(interval)

}
// --- 結束新增 ---

// --- 新增：計算所有章節的發布時間表 ---
fn calculate_chapter_schedules(
    initial_datetime: chrono::DateTime<chrono_tz::Tz>,
    interval: ScheduleInterval,
    count: usize, // 章節數量
) -> Result<Vec<ScheduleInfo>> {
    let interval_duration = interval.to_duration()?; // 獲取 chrono::Duration
    let mut schedules = Vec::with_capacity(count);

    println!("計算得到的各章節預定發布時間 (台北時間):");
    for i in 0..count {
        // 計算當前章節的時間 = 初始時間 + i * 間隔
        // 使用 checked_add_signed 防止溢出
        let current_datetime = initial_datetime.checked_add_signed(interval_duration * (i as i32))
            .ok_or_else(|| anyhow!("計算第 {} 個章節時間時發生溢出", i + 1))?;

        // 格式化回 ScheduleInfo 所需的字符串
        let date_str = current_datetime.format("%Y-%m-%d").to_string();
        let hour_str = current_datetime.format("%H").to_string();
        // --- 重要修正：確保分鐘也是兩位數，並且處理非 00/30 的情況 (雖然這裡輸入是 00/30) ---
        // 格式化為兩位數分鐘
        let minute_str = current_datetime.format("%M").to_string();
        // 可選：如果嚴格要求輸出必須是 00 或 30，可以在這裡加邏輯，但這可能與計算結果不符
        // if minute_str != "00" && minute_str != "30" {
        //     println!("警告：計算得到的第 {} 章分鐘為 {}，非 00 或 30。", i + 1, minute_str);
        // }

        println!("  - 章節 {}: {}", i + 1, current_datetime.format("%Y-%m-%d %H:%M"));

        schedules.push(ScheduleInfo {
            date: date_str,
            hour: hour_str,
            minute: minute_str, // 使用計算得到的分鐘字符串
        });
    }
    Ok(schedules)
}
// --- 結束新增 ---

// --- 新增：決定發布時間表的函數 ---
pub fn determine_schedules(chapter_count: usize) -> Result<Vec<Option<ScheduleInfo>>> {
    let mut schedules: Vec<Option<ScheduleInfo>> = vec![None; chapter_count]; // 初始化為都不預約

    if chapter_count == 0 {
        println!("沒有找到要發布的章節。");
        return Ok(vec![]); // 返回空 Vec
    }

    let schedule_choice = get_input("是否需要預約發文 (y/n，默認 n): ").to_lowercase();

    if schedule_choice == "y" {
        if chapter_count > 1 {
            // --- 多章節情況 ---
            let advanced_choice = get_input("檢測到多個章節，是否使用進階預約發文 (按固定間隔安排) (y/n，默認 n): ").to_lowercase();
            if advanced_choice == "y" {
                // --- 進階預約 ---
                println!("--- 設置進階預約 ---");
                let initial_datetime = prompt_for_initial_datetime()?; // 獲取初始時間點
                let interval = prompt_for_interval()?; // 獲取間隔

                // --- 新增：檢查間隔是否為零 ---
                if interval.days == 0 && interval.hours == 0 && interval.minutes == 0 {
                    // --- 間隔為零，跳轉到統一時間邏輯 ---
                    println!("檢測到時間間隔為零，將為所有章節設置統一的初始預約時間。");
                    let single_schedule = ScheduleInfo {
                         date: initial_datetime.format("%Y-%m-%d").to_string(),
                         hour: initial_datetime.format("%H").to_string(),
                         minute: initial_datetime.format("%M").to_string(),
                    };
                    schedules.fill(Some(single_schedule)); // 所有章節使用相同時間
                    println!("所有 {} 個章節將預約在 {}", chapter_count, initial_datetime.format("%Y-%m-%d %H:%M %Z"));
                    // --- 跳轉邏輯結束 ---
                } else {
                    // --- 間隔不為零，執行原計算邏輯 ---
                    let calculated_schedules = calculate_chapter_schedules(initial_datetime, interval, chapter_count)?; // 計算時間表
                    schedules = calculated_schedules.into_iter().map(Some).collect();
                    // --- 原計算邏輯結束 ---
                }
                 // --- 結束新增的檢查 ---
                 
            } else {
                // --- 多章節，但使用統一時間 ---
                println!("--- 為所有章節設置統一預約時間 ---");
                let single_datetime = prompt_for_initial_datetime()?; // 獲取單個時間點
                // 將 DateTime<Taipei> 轉換為 ScheduleInfo
                let single_schedule = ScheduleInfo {
                     date: single_datetime.format("%Y-%m-%d").to_string(),
                     hour: single_datetime.format("%H").to_string(),
                     minute: single_datetime.format("%M").to_string(), // 同樣格式化分鐘
                };
                schedules.fill(Some(single_schedule)); // 所有章節使用相同時間
                println!("所有 {} 個章節將預約在 {}", chapter_count, single_datetime.format("%Y-%m-%d %H:%M %Z"));
            }
        } else {
            // --- 單章節情況 ---
            println!("--- 設置單章節預約時間 ---");
             let single_datetime = prompt_for_initial_datetime()?; // 獲取單個時間點
             let single_schedule = ScheduleInfo {
                 date: single_datetime.format("%Y-%m-%d").to_string(),
                 hour: single_datetime.format("%H").to_string(),
                 minute: single_datetime.format("%M").to_string(), // 格式化分鐘
             };
             // 確保 schedules 向量至少有一個元素
             if !schedules.is_empty() {
                schedules[0] = Some(single_schedule); // 設置第一個 (也是唯一一個)
             }
        }
    } else {
        println!("不進行預約發布，將立即發布所有章節。");
        // schedules 保持全為 None
    }

    Ok(schedules) // 返回最終的時間表向量
}
// --- 結束替換/新增 ---

pub async fn processor(work_dir: &str) -> Result<(Config, Vec<Chapter>)>{
    let book_path = get_input("請輸入書籍路徑: ");
    let config = Config::build(&book_path)?;

    let selected = get_input("請輸入需要處理的章節範圍: ");
    let selected_indices = parse_ranges(&selected)?;
    let chapter_map =
        parse_chapters(&book_path, &config.chapter_separator, &selected_indices, work_dir)?;

    if config.encrypt {
        let executable = get_executable_name(EXECUTABLE_FILE);
        let chapters = encrypt_chapters(chapter_map, &config.book_id, work_dir, &executable).await?;
        Ok((config, chapters))
    } else {
        let chapters = no_encrypt_chapters(chapter_map, work_dir)?;
        Ok((config, chapters))
    }
}

async fn encrypt_chapters(
    chapter_map: BTreeMap<usize, (String, PathBuf)>,
    book_id: &str,
    work_dir: &str,
    executable: &str,
) -> Result<Vec<Chapter>> {
    let work_dir = Arc::new(work_dir.to_string());
    let executable = Arc::new(executable.to_string());

    let mut tasks = Vec::new();

    for (index, (title, file_path)) in chapter_map {
        let work_dir = Arc::clone(&work_dir);
        let executable = Arc::clone(&executable);

        let task = tokio::spawn(async move {
            let file_name = extract_file_name(&file_path)?;

            let chapter = Chapter {
                index,
                title,
                target_path: format!("{}/encrypted/{}.txt", work_dir, file_name),
                font_path: Some(format!("{}/font/{}.woff", work_dir, file_name)),
                css_url: None,
            };

            process_chapter(file_path.to_str().ok_or(anyhow!("路徑轉換失敗"))?, &executable, &chapter).await?;

            Ok::<_, anyhow::Error>(chapter)
        });

        tasks.push(task);
    }

    let mut chapters = Vec::new();
    for task in tasks {
        let chapter = task.await??;
        // chapter.set_css_url(&book_id).await?;
        chapters.push(chapter);
    }

    Chapter::collect_css_url(book_id, &mut chapters).await?;

    commit::git_push()?;

    Ok(chapters)
}

fn no_encrypt_chapters(
    chapter_map: BTreeMap<usize, (String, PathBuf)>,
    work_dir: &str,)
    ->Result<Vec<Chapter>> {
    let mut chapters_vec = Vec::new();
    for (index, (title, file_path)) in chapter_map {
        let chapter = Chapter {
            index,
            title,
            target_path: format!("{}/{}.txt", work_dir, extract_file_name(&file_path)?),
            font_path: None,
            css_url: None,
        };

        chapters_vec.push(chapter);
    }
    Ok(chapters_vec)
}

fn parse_ranges(input: &str) -> Result<Vec<(usize, usize)>> {
    let re = Regex::new(r"^\s*(\d+(-\d*)?\s*,\s*)*\d+(-\d*)?\s*$")?;
    if !re.is_match(input) {
        return Err(anyhow!("章節範圍格式錯誤"));
    }

    let mut ranges = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.contains('-') {
            let range: Vec<&str> = part.split('-').collect();
            let start = range[0].parse::<usize>().unwrap_or(1);
            let end = range.get(1)
                .and_then(|&s| s.parse::<usize>().ok()).unwrap_or(usize::MAX);
            ranges.push((start, end));
        } else if let Ok(num) = part.parse::<usize>() {
            ranges.push((num, num));
        }
    }
    ranges.sort_unstable_by_key(|&(start, _)| start);
    Ok(ranges)
}
fn extract_file_name(file_path: &PathBuf) -> Result<String> {
    file_path.file_stem()
        .ok_or(anyhow!("文件路徑錯誤"))
        .map(|stem| stem.to_string_lossy().to_string())
}


async fn process_chapter(file_path: &str, executable: &str, chapter: &Chapter) -> Result<()> {
    println!("正在加密章節: {}", chapter.title);
    let output = tokio::process::Command::new(executable)
        .args(["-f", file_path, "-s", &chapter.target_path, "-t",
            &chapter.font_path.as_ref().ok_or(anyhow!("字體路徑錯誤"))?])
        .output()
        .await
        .context("執行加密程序失敗")?;
    if !output.status.success() {
        return Err(anyhow!("加密失敗: {}", String::from_utf8_lossy(&output.stderr)));
    }
    println!("章節加密完成: {}", chapter.title);
    Ok(())
}

fn save_chapter(work_path: &Path, file_name: usize, content: &str) -> Result<PathBuf> {
    let chapter_path = work_path.join(format!("{}.txt", file_name));
    let mut file = File::create(&chapter_path).context("無法創建章節文件")?;
    file.write_all(content.as_bytes()).context("寫入章節文件失敗")?;
    Ok(chapter_path)
}

fn finish_chapter(
    chapter_num: &mut usize,
    current_title: &mut String,
    current_content: &mut String,
    current_range: &mut Option<(usize, usize)>,
    chapters: &mut BTreeMap<usize, (String, PathBuf)>,
    work_path: &Path,
    range_iter: &mut impl Iterator<Item = (usize, usize)>,
) -> Result<()> {
    *chapter_num += 1;
    if let Some((start, end)) = *current_range {
        if (start..=end).contains(&*chapter_num) {
            let chapter_path = save_chapter(work_path, *chapter_num, current_content)?;
            chapters.insert(*chapter_num, (current_title.clone(), chapter_path));
        }
        if *chapter_num >= end {
            *current_range = range_iter.next();
        }
    }
    current_title.clear();
    current_content.clear();
    Ok(())
}

fn parse_chapters(
    book_path: &str,
    separator: &str,
    ranges: &[(usize, usize)],
    work_dir: &str,
) -> Result<BTreeMap<usize, (String, PathBuf)>> {
    let file = File::open(book_path).context("無法打開文件")?;
    let reader = BufReader::new(file);
    let work_path = Path::new(work_dir);
    let mut chapters = BTreeMap::new();

    let mut chapter_num = 0;
    let mut current_title = String::new();
    let mut current_content = String::new();
    let mut range_iter = ranges.iter().copied();
    let mut current_range = range_iter.next();

    for line in reader.lines() {
        let line = line.context("讀取行失敗")?;
        let trimmed = line.trim();

        if trimmed == separator {
            finish_chapter(
                &mut chapter_num,
                &mut current_title,
                &mut current_content,
                &mut current_range,
                &mut chapters,
                work_path,
                &mut range_iter,
            )?;
            continue;
        }

        if trimmed.is_empty() && (current_title.is_empty() || current_content.is_empty()) {
            continue;
        }

        if current_title.is_empty() {
            current_title = trimmed.to_string();
        } else {
            current_content.push_str(&line);
            current_content.push('\n');
        }
    }

    if !current_title.is_empty() {
        finish_chapter(
            &mut chapter_num,
            &mut current_title,
            &mut current_content,
            &mut current_range,
            &mut chapters,
            work_path,
            &mut range_iter,
        )?;
    }
    Ok(chapters)
}
