//! Rocktier OCR — make scanned PDFs searchable.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
/// 家族内唯一的产品标识，用作试用记录的副存储命名空间。
///
/// 必须与 `tauri.conf.json` 的 `bundle.identifier` 逐字一致 ——
/// 副存储按它分文件，改了会导致老用户的试用记录读不到（等于白送 7 天）。
/// 改动时两处必须同步。
pub const APP_KEY: &str = "Rocktier.RocktierOCR";

// 授权：试用状态与回执验签。写命令的拦截在 commands.rs，界面在 LicenseDialog。
mod license;
mod trial;
mod menu;
mod pdf;

use tauri::Emitter;

/* ── 授权：试用与激活（见 license.rs 的模块说明；接线照 MD dc8e7b7 / PDF 范本）── */

/// 试用与授权状态的落盘目录。由 `setup()` 注入。
///
/// 用全局而不是给写命令各加一个参数：那会让命令签名多一个与业务无关的参数，
/// 而它也不是业务状态，读它不需要与转换状态同步。
static LICENSE_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// 供闸门发事件用。setup 注入；即使没注入也照样能拦截，只是界面不会自动弹窗。
static APP_HANDLE: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 当前授权状态。
///
/// 目录未注入（setup 失败）时按"试用中、满额天数"处理 —— 失败方向刻意选**放行**：
/// 一个取不到的目录不该变成一次锁死。
fn current_license() -> license::Status {
    let Some(dir) = LICENSE_DIR.get() else {
        return license::Status::Trialing { days_left: license::TRIAL_DAYS };
    };
    let now = now_secs();
    /* 试用起点双写（AppData + 副存储）并按机器指纹判定，
       见 trial.rs 的模块说明。app_key 用 bundle identifier ——
       家族内唯一，避免两个产品的副存储互相覆盖。 */
    let started = trial::ensure_started(
        dir,
        APP_KEY,
        now,
        &trial::machine_fingerprint(),
    );
    // 只认本单品与全家桶的回执：别人的回执即使验签通过，也不是本应用的授权。
    let receipt = license::read_valid_receipt(dir, license::PUBLIC_KEY_B64).filter(license::accepts);
    license::status_from(Some(started), receipt.as_ref(), now)
}

/// 写操作的统一闸门。
///
/// 在**命令层**拦，而不是在每个界面路径上判断：界面路径会随功能增长而增加，漏掉一条
/// 就是一道缝；命令层是所有写操作的必经之路。OCR 的写命令只有 ocr_export_pdf 与
/// ocr_export_txt（写到文件的两条导出）；ocr_process 只做识别不落盘，不拦
/// （FAMILY-LICENSE.md §2 的 OCR 行）。
///
/// 错误码固定为 `LICENSE_EXPIRED`，前端凭它弹购买/激活框。
fn ensure_write_allowed() -> Result<(), String> {
    if current_license().allows_write(license::enforced()) {
        return Ok(());
    }
    // 让界面主动知道"被拦下了"，而不是在每个动作的 catch 里各判一次错误码 ——
    // 那种写法漏掉一处，用户看到的就只是一个没有解释的失败。
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit("license-expired", ());
    }
    Err("LICENSE_EXPIRED".to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseInfo {
    /// `trial` / `expired` / `licensed`。
    pub status: String,
    /// 仅 `trial` 时有意义。
    pub days_left: i64,
    /// 仅 `licensed` 时有值（`OC` 单品 / `FL` 全家桶）。
    pub product: Option<String>,
    /// 当前是否真的会拦截写操作（渠道 + 公钥 + 总开关三者决定）。
    pub enforcing: bool,
    /// `direct`（官网直链）/ `store`（微软商店）。
    pub channel: String,
    /// 本构建是否已配置验签公钥。
    ///
    /// 没配置时**任何人都激活不了**（回执必然验不过）。界面据此如实说明，而不是
    /// 拿"激活码未被接受"去搪塞一位已经付过钱的用户。
    pub activation_configured: bool,
}

fn license_info() -> LicenseInfo {
    let status = current_license();
    LicenseInfo {
        status: status.as_str().to_string(),
        days_left: match &status {
            license::Status::Trialing { days_left } => *days_left,
            _ => 0,
        },
        product: match &status {
            license::Status::Licensed { product } => Some(product.clone()),
            _ => None,
        },
        enforcing: license::enforced(),
        channel: license::channel().to_string(),
        activation_configured: !license::PUBLIC_KEY_B64.trim().is_empty(),
    }
}

/// 供界面展示：剩余试用天数 / 是否已激活 / 当前渠道。
///
/// ⚠️ 不能加 `pub`：本文件的命令定义在 crate 根（bin crate），而 `#[tauri::command]`
/// 对 `pub` 命令会生成 `#[macro_export]`，宏被提升到 crate 根后与本地定义同名冲突
/// （E0255，MD 模板验证发现的坑）。commands.rs 里的命令不受影响 —— 它们在子模块。
#[tauri::command]
async fn license_status() -> Result<LicenseInfo, String> {
    Ok(license_info())
}

/// 保存服务端签出的回执并立即验签。
///
/// 联网换回执的那一步在**前端**做（`fetch` 到 rocktier.com/api/activate），
/// 为的是不引入 HTTP 客户端依赖；但**验签与落盘必须在这里** —— 前端拿到的只是一段
/// 待验的字符串，能证明它有效与否的只有公钥。
#[tauri::command]
async fn store_receipt(signed: String) -> Result<LicenseInfo, String> {
    let dir = LICENSE_DIR
        .get()
        .ok_or_else(|| "no app data directory".to_string())?;
    let trimmed = signed.trim();
    let receipt = license::verify_receipt(trimmed, license::PUBLIC_KEY_B64)?;

    // 其它单品的码虽然签名有效，但**不属于**本应用 —— 而且不要落盘：落下去以后
    // 会被当成有效回执读回来，等于自己给自己开后门。
    if !license::accepts(&receipt) {
        return Err("LICENSE_WRONG_PRODUCT".to_string());
    }

    license::save_receipt(dir, trimmed)?;
    Ok(license_info())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(commands::AppState::default())
        .setup(|app| {
            // 授权状态的落盘目录。取不到就留空，current_license() 会按"不拦截"处理
            // —— 宁可少拦一次，也不能因为一个目录取不到把用户锁在外面（与 MD/PDF 同款）。
            use tauri::Manager as _;
            if let Ok(dir) = app.path().app_data_dir() {
                let _ = LICENSE_DIR.set(dir);
            }
            let _ = APP_HANDLE.set(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ocr_process,
            commands::ocr_export_pdf,
            commands::ocr_export_txt,
            commands::ocr_cancel,
            menu::build_menu,
            menu::open_url,
            license_status,
            machine_fingerprint,
            store_receipt,
        ])
        // 家族菜单规范：自定义项 → 前端 menu-action 事件（复用现有动作链，
        // 禁用态与运行中守卫都在前端的按钮逻辑里）。OCR 没有未保存内容，
        // 退出不走关窗守卫，直接 app.exit(0)。
        .on_menu_event(|app, event| {
            if event.id().0.as_str() == "quit" {
                app.exit(0);
                return;
            }
            let _ = app.emit("menu-action", event.id().0.as_str());
        })
        .run(tauri::generate_context!())
        .expect("error while running Rocktier OCR");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验收（FAMILY-LICENSE.md §6）：把试用起始时间改到过期后，
    /// 写命令的闸门 `ensure_write_allowed` 必须返回含 `LICENSE_EXPIRED` 的错误。
    /// 构造法照 license.rs 既有测试：直接往状态目录里写起始时间戳。
    #[test]
    fn an_expired_trial_makes_the_write_gate_return_license_expired() {
        // 闸门真实生效的前提：ENFORCE + 直链渠道 + 公钥已配（测试构建三条都成立，
        // 与 license.rs 的 a_configured_key_in_the_direct_channel_engages_the_gate 同源）。
        assert!(
            license::enforced(),
            "测试前提：ENFORCE=true、直链渠道、公钥已配时 enforced() 应为 true"
        );

        let dir = std::env::temp_dir().join(format!("rt-oc-license-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // LICENSE_DIR 是进程级单例：本测试是唯一设置它的测试。若将来有人加第二条，
        // 后到的 set 会失败 —— 那时合并两条测试，不要让闸门测试静默跑偏。
        if LICENSE_DIR.set(dir.clone()).is_err() {
            panic!("LICENSE_DIR 已被其他测试设置，闸门测试无法控制状态目录");
        }

        // 试用期第一天：写操作放行。
        let now = now_secs();
        // 文件名即 license.rs 的 STATE_FILE（模块私有常量，这里按值写）。
        std::fs::write(dir.join("state.bin"), now.to_string()).unwrap();
        assert_eq!(ensure_write_allowed(), Ok(()), "试用期内写操作必须放行");

        // 把试用起始时间改到 30 天前：状态 = Expired，必须被拦，错误码固定。
        std::fs::write(dir.join("state.bin"), (now - 30 * 86_400).to_string()).unwrap();
        let err = ensure_write_allowed().unwrap_err();
        assert!(
            err.contains("LICENSE_EXPIRED"),
            "过期后写操作应返回 LICENSE_EXPIRED，实际为 {err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
