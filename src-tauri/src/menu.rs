//! Rocktier OCR 原生菜单（家族规范《Rocktier家族软件通用准则.md》第十三章）。
//!
//! 结构对齐家族标准：应用 / 文件 / 编辑 / 显示 / 窗口 / 帮助。
//! 由前端挂载后按当前 UI 语言调用 `build_menu(lang)`，语言切换时重建；
//! 自定义项的点击经 main.rs 的 `on_menu_event` 转成 "menu-action" 事件发给前端；
//! 预定义项（撤销/拷贝/最小化等）由系统自动本地化并自带快捷键。
//!
//! OCR 的界面动作集很小，特有项只有：打开…、导出 TXT…、导出 PDF…、关闭窗口
//! （文件段）和切换语言、切换主题（显示段，按规范不给单键快捷键）。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::AppHandle;

/// 构建并按当前语言安装原生菜单。
pub fn build_app_menu(app: &AppHandle, lang: &str) -> tauri::Result<()> {
    let zh = lang.starts_with("zh");
    let l = |zhv: &'static str, en: &'static str| if zh { zhv } else { en };

    let open_i = MenuItem::with_id(app, "open", l("打开…", "Open…"), true, Some("CmdOrCtrl+O"))?;
    let export_txt_i = MenuItem::with_id(
        app,
        "export-txt",
        l("导出 TXT…", "Export TXT…"),
        true,
        None::<&str>,
    )?;
    let export_pdf_i = MenuItem::with_id(
        app,
        "export-pdf",
        l("导出 PDF…", "Export PDF…"),
        true,
        None::<&str>,
    )?;

    let app_menu = Submenu::with_items(
        app,
        "Rocktier OCR",
        true,
        &[
            &PredefinedMenuItem::about(
                app,
                Some(l("关于 Rocktier OCR", "About Rocktier OCR")),
                None,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", l("退出", "Quit"), true, Some("CmdOrCtrl+Q"))?,
        ],
    )?;

    let file_menu = Submenu::with_items(
        app,
        l("文件", "File"),
        true,
        &[
            &open_i,
            &PredefinedMenuItem::separator(app)?,
            &export_txt_i,
            &export_pdf_i,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        l("编辑", "Edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let lang_i = MenuItem::with_id(
        app,
        "toggle-lang",
        l("切换语言", "Switch Language"),
        true,
        None::<&str>,
    )?;
    let theme_i = MenuItem::with_id(
        app,
        "toggle-theme",
        l("切换日夜模式", "Toggle Theme"),
        true,
        None::<&str>,
    )?;
    let view_menu = Submenu::with_items(app, l("显示", "View"), true, &[&lang_i, &theme_i])?;

    let window_menu = Submenu::with_items(
        app,
        l("窗口", "Window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    let site_i = MenuItem::with_id(app, "website", l("官方网站", "Website"), true, None::<&str>)?;
    let mail_i = MenuItem::with_id(app, "feedback", l("反馈", "Feedback"), true, None::<&str>)?;
    let help_menu = Submenu::with_items(app, l("帮助", "Help"), true, &[&site_i, &mail_i])?;

    let menu = Menu::with_items(
        app,
        &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu, &help_menu],
    )?;
    app.set_menu(menu)?;
    Ok(())
}

/// 前端挂载后（以及语言切换时）调用，按 UI 语言（"zh" / "en"）构建菜单。
#[tauri::command]
pub fn build_menu(app: AppHandle, lang: String) -> Result<(), String> {
    build_app_menu(&app, &lang).map_err(|e| e.to_string())
}

/// 帮助菜单里的外链（官网 / 反馈邮箱），白名单防止任意 URL。
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    const ALLOWED: [&str; 3] = [
        "https://rocktier.com/",
        "https://www.rocktier.com/",
        "mailto:",
    ];
    if !ALLOWED.iter().any(|p| url.starts_with(p)) {
        return Err(format!("blocked url: {url}"));
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}
