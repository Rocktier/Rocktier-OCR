//! Rocktier OCR 原生菜单（家族规范《Rocktier家族软件通用准则.md》第十三章）。
//!
//! 结构对齐家族标准：应用 / 文件 / 编辑 / 显示 / 窗口 / 帮助。
//! 由前端挂载后按当前 UI 语言调用 `build_menu(lang)`，语言切换时重建；
//! 自定义项的点击经 main.rs 的 `on_menu_event` 转成 "menu-action" 事件发给前端；
//! 预定义项（撤销/拷贝/最小化等）由系统自动本地化并自带快捷键。
//!
//! OCR 的界面动作集很小，特有项只有：打开…、导出 TXT…、导出 PDF…、关闭窗口
//! （文件段）和切换语言、切换主题（显示段，按规范不给单键快捷键）。

use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::AppHandle;

/// 构建并按当前语言安装原生菜单。
/// Menu labels for one language.
///
/// Same approach as the other six products' menus: a struct per language
/// instead of widening the old `l(zh, en)` closure to eight arguments — with
/// eight positional string arguments, swapping `ja` and `ko` compiles cleanly
/// and silently shows the wrong language. One field per call site makes that a
/// compile error.
///
/// OCR also has 8 interface languages, so a menu that was zh/en only meant
/// a Japanese interface with a Chinese-or-English menu bar.
///
/// Unknown codes fall back to English rather than panicking, so a stale
/// `localStorage` value degrades to a usable menu.
struct MenuStrings {
    open: &'static str,
    export_pdf: &'static str,
    export_txt: &'static str,
    file: &'static str,
    edit: &'static str,
    view: &'static str,
    switch_language: &'static str,
    toggle_theme: &'static str,
    window: &'static str,
    help: &'static str,
    website: &'static str,
    feedback: &'static str,
    about: &'static str,
    quit: &'static str,
}

impl MenuStrings {
    fn for_lang(lang: &str) -> Self {
        // Primary subtag, so "zh-CN" and "zh-Hans" both land on zh.
        let code = lang.split(['-', '_']).next().unwrap_or("");
        match code {
            "zh" => Self {
                open: "打开…", export_pdf: "导出 PDF…", export_txt: "导出 TXT…",
                file: "文件", edit: "编辑", view: "显示", switch_language: "切换语言",
                toggle_theme: "切换日夜模式", window: "窗口", help: "帮助",
                website: "官方网站", feedback: "反馈", about: "关于 Rocktier OCR",
                quit: "退出",
            },
            "ja" => Self {
                open: "開く…", export_pdf: "PDF を書き出し…", export_txt: "TXT を書き出し…",
                file: "ファイル", edit: "編集", view: "表示", switch_language: "言語を切り替え",
                toggle_theme: "テーマを切り替え", window: "ウインドウ", help: "ヘルプ",
                website: "公式サイト", feedback: "フィードバック", about: "Rocktier OCR について",
                quit: "終了",
            },
            "ko" => Self {
                open: "열기…", export_pdf: "PDF 내보내기…", export_txt: "TXT 내보내기…",
                file: "파일", edit: "편집", view: "보기", switch_language: "언어 전환",
                toggle_theme: "테마 전환", window: "창", help: "도움말",
                website: "공식 웹사이트", feedback: "피드백", about: "Rocktier OCR 정보",
                quit: "종료",
            },
            "de" => Self {
                open: "Öffnen…", export_pdf: "Als PDF exportieren…", export_txt: "Als TXT exportieren…",
                file: "Datei", edit: "Bearbeiten", view: "Ansicht", switch_language: "Sprache wechseln",
                toggle_theme: "Design wechseln", window: "Fenster", help: "Hilfe",
                website: "Website", feedback: "Feedback", about: "Über Rocktier OCR",
                quit: "Beenden",
            },
            "es" => Self {
                open: "Abrir…", export_pdf: "Exportar a PDF…", export_txt: "Exportar a TXT…",
                file: "Archivo", edit: "Editar", view: "Ver", switch_language: "Cambiar idioma",
                toggle_theme: "Cambiar tema", window: "Ventana", help: "Ayuda",
                website: "Sitio web", feedback: "Comentarios", about: "Acerca de Rocktier OCR",
                quit: "Salir",
            },
            "pt" => Self {
                open: "Abrir…", export_pdf: "Exportar para PDF…", export_txt: "Exportar para TXT…",
                file: "Arquivo", edit: "Editar", view: "Exibir", switch_language: "Mudar idioma",
                toggle_theme: "Alternar tema", window: "Janela", help: "Ajuda",
                website: "Site", feedback: "Comentários", about: "Sobre o Rocktier OCR",
                quit: "Sair",
            },
            "ar" => Self {
                open: "فتح…", export_pdf: "تصدير PDF…", export_txt: "تصدير TXT…",
                file: "ملف", edit: "تحرير", view: "عرض", switch_language: "تغيير اللغة",
                toggle_theme: "تبديل المظهر", window: "نافذة", help: "مساعدة",
                website: "الموقع", feedback: "ملاحظات", about: "حول Rocktier OCR",
                quit: "إنهاء",
            },
            // English is both the family default and the fallback.
            _ => Self {
                open: "Open…", export_pdf: "Export PDF…", export_txt: "Export TXT…",
                file: "File", edit: "Edit", view: "View", switch_language: "Switch Language",
                toggle_theme: "Toggle Theme", window: "Window", help: "Help",
                website: "Website", feedback: "Feedback", about: "About Rocktier OCR",
                quit: "Quit",
            },
        }
    }
}

pub fn build_app_menu(app: &AppHandle, lang: &str) -> tauri::Result<()> {
    let m = MenuStrings::for_lang(lang);

    let open_i = MenuItem::with_id(app, "open", m.open, true, Some("CmdOrCtrl+O"))?;
    let export_txt_i = MenuItem::with_id(
        app,
        "export-txt",
        m.export_txt,
        true,
        None::<&str>,
    )?;
    let export_pdf_i = MenuItem::with_id(
        app,
        "export-pdf",
        m.export_pdf,
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
                Some(m.about),
                                Some(AboutMetadata {
                    version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    copyright: Some("Copyright 2026 Rocktier".to_string()),
                    ..Default::default()
                }),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", m.quit, true, Some("CmdOrCtrl+Q"))?,
        ],
    )?;

    let file_menu = Submenu::with_items(
        app,
        m.file,
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
        m.edit,
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
        m.switch_language,
        true,
        None::<&str>,
    )?;
    let theme_i = MenuItem::with_id(
        app,
        "toggle-theme",
        m.toggle_theme,
        true,
        None::<&str>,
    )?;
    let view_menu = Submenu::with_items(app, m.view, true, &[&lang_i, &theme_i])?;

    let window_menu = Submenu::with_items(
        app,
        m.window,
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::fullscreen(app, None)?,
        ],
    )?;

    let site_i = MenuItem::with_id(app, "website", m.website, true, None::<&str>)?;
    let mail_i = MenuItem::with_id(app, "feedback", m.feedback, true, None::<&str>)?;
    let help_menu = Submenu::with_items(app, m.help, true, &[&site_i, &mail_i])?;

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
