#!/bin/bash
# Rocktier OCR —— Mac App Store 打包（两段式）
#
#   用法:
#     bash scripts/build-appstore.sh pack                 # 编 store 渠道 universal .app（CI 或本机有 Rust 时）
#     bash scripts/build-appstore.sh pack --build-num 812 # 指定 CFBundleVersion
#     bash scripts/build-appstore.sh sign --zip <路径>    # 本机：嵌描述文件 + 签名 + 出 .pkg
#     bash scripts/build-appstore.sh sign --app <路径>
#
#   环境变量:
#     MAS_PROFILE       描述文件路径（默认自动按 bundle id 匹配）
#     MAS_SIGN_ID       覆盖 .app 签名身份（默认从钥匙串探测 Apple Distribution）
#     MAS_INSTALLER_ID  覆盖 .pkg 签名身份（默认从钥匙串探测 Installer 证书）
#     BUILD_NUM         pack 的 CFBundleVersion（默认 git 提交数；CI 传 run_number）
#
# ─────────────────────────────────────────────────────────────────────
# 为什么拆成 pack / sign 两段
# ─────────────────────────────────────────────────────────────────────
# 1) 渠道是**编译期常量**：src-tauri/src/license.rs 里
#      option_env!("ROCKTIER_CHANNEL") == Some("store")
#    决定是否启用自研付费墙。直发渠道编出来的 .app 无法靠重签变成商店渠道版，
#    二进制里已经写死了 —— 必须重新编译。带付费墙的包进 App Store 会被审核指南
#    3.1.1（In-App Purchase）直接拒。
#
# 2) 本机没有 Rust 工具链（家族准则 §14 铁律 #7）：编译放 CI，签名留本机。
#    这样私钥永远不出本机，CI 也不需要任何证书 secrets —— 与直发轨的
#    APPLE_CERT_P12（Developer ID）那套完全解耦，两套证书不可混用。
#
# 3) 本机有 productbuild / codesign（CLT 已装），产 .pkg 这一段完全够用。
#
# ─────────────────────────────────────────────────────────────────────
# 两个已实测的坑（别再踩）
# ─────────────────────────────────────────────────────────────────────
# · codesign 传 --entitlements 时**不会**自动从 embedded.provisionprofile 补
#   com.apple.application-identifier / com.apple.developer.team-identifier。
#   缺这两项的包 codesign --verify 照样绿，只在 ASC 上传阶段被拒 —— 所以本脚本
#   从描述文件里读出来显式注入，并逐项断言。
# · 不要加 --timestamp：MAS 签名不需要可信时间戳，而本机点该参数会去连 Apple
#   时间戳服务，网络不通时会整条命令挂死（实测被 SIGTERM 掐掉，还留下 .cstemp
#   残骸让下一次 --verify 报「a sealed resource is missing」）。
#
# OCR 特有：bundle.resources 里带 libpdfium.dylib（进 Contents/Resources），
# sign 段的 dylib 循环会把它一并按 Apple Distribution 重签；ONNX 模型是纯数据
# 文件，不需要签名。

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$SCRIPT_DIR"

VERSION=$(node -p "require('./package.json').version")
BUNDLE_ID=$(node -p "require('./src-tauri/tauri.conf.json').identifier")
APP_NAME=$(node -p "require('./src-tauri/tauri.conf.json').productName")
OUT_DIR="$SCRIPT_DIR/build/appstore"

TMP_PATHS=()
cleanup() {
    local p
    for p in ${TMP_PATHS[@]+"${TMP_PATHS[@]}"}; do
        [ -n "$p" ] && rm -rf "$p"
    done
}
trap cleanup EXIT

die() { echo "❌ $*" >&2; exit 1; }
say() { echo "   $*"; }

# 注意：macOS 的 mktemp 要求模板**以 XXXXXX 结尾**。写成
#   mktemp /tmp/xxx-XXXXXX.plist
# 不会随机化 —— 它把整串当固定路径，第一次成功、第二次报 File exists。
# 所以这里一律不带后缀，扩展名对后续 plist/codesign 都无所谓。
#
# 也**不能**写成 `f=$(tmpfile ...)`：命令替换会开子 shell，TMP_PATHS+= 的修改
# 传不回父 shell，trap 就清不掉临时文件。统一用 TMP_LAST 回传。
TMP_LAST=""
tmpfile() {
    TMP_LAST=$(mktemp "$1")
    TMP_PATHS+=("$TMP_LAST")
}

tmpdir() {
    TMP_LAST=$(mktemp -d "$1")
    TMP_PATHS+=("$TMP_LAST")
}

# 读 Info.plist 的字符串键。
# 不用 plutil -extract：键缺失时它把报错写进 stdout，`$(...) || echo ""` 会把
# 错误文本当成取值带出来（实测「分类」一行打印成了一整条 plutil 报错）。
plist_str() {
    /usr/libexec/PlistBuddy -c "Print :$2" "$1" 2>/dev/null || true
}

# ─────────────────────────────────────────────────────────────────────
# pack：编 store 渠道 universal（arm64 + x86_64）.app
# ─────────────────────────────────────────────────────────────────────
cmd_pack() {
    local build_num="$1"

    # 无条件设死：本脚本唯一合理的渠道就是 store。
    # 这样「CI 忘了传环境变量」不会静默产出一个带付费墙的包（家族准则：不靠发布纪律）。
    export ROCKTIER_CHANNEL=store

    command -v npx >/dev/null 2>&1 || die "缺 npx（需要 Node.js）"

    echo "=============================================="
    echo "  MAS pack：store 渠道 universal .app"
    echo "=============================================="
    say "产品:     ${APP_NAME}"
    say "版本:     ${VERSION}（build ${build_num}）"
    say "渠道:     ROCKTIER_CHANNEL=${ROCKTIER_CHANNEL}"
    say "配置:     src-tauri/tauri.appstore.conf.json"

    # bundleVersion 只在 CI 注入：写进临时合并配置，不改仓库里的文件
    local tmp_conf
    tmpfile /tmp/rocktier-ocr-mas-conf-XXXXXX
    tmp_conf="$TMP_LAST"
    MAS_TMP_CONF="$tmp_conf" MAS_BUILD_NUM="$build_num" node -e '
      const fs = require("fs");
      const conf = JSON.parse(fs.readFileSync("src-tauri/tauri.appstore.conf.json", "utf8"));
      conf.bundle = conf.bundle || {};
      conf.bundle.macOS = conf.bundle.macOS || {};
      conf.bundle.macOS.bundleVersion = process.env.MAS_BUILD_NUM;
      fs.writeFileSync(process.env.MAS_TMP_CONF, JSON.stringify(conf, null, 2) + "\n");
    '

    echo ""
    echo "📦 Tauri build（arm64，只产 .app）..."
    npm run tauri -- build \
        --config "$tmp_conf" \
        --target aarch64-apple-darwin \
        --bundles app

    local bundle_dir="src-tauri/target/aarch64-apple-darwin/release/bundle/macos"
    local app
    app=$(find "$bundle_dir" -maxdepth 1 -name '*.app' -print -quit 2>/dev/null)
    [ -n "$app" ] || die "构建后找不到 .app（${bundle_dir}）"

    # 架构断言：arm64 必须有。x86_64 缺是**预期行为**（ort-sys rc.15 没有
    # x86_64 预编译 ONNX Runtime，universal 必然失败），与直发轨一致。
    local bin archs
    bin=$(find "$app/Contents/MacOS" -maxdepth 1 -type f -perm -111 -print -quit)
    [ -n "$bin" ] || die "找不到可执行文件"
    archs=$(lipo -archs "$bin" 2>/dev/null || echo "")
    say "架构: ${archs}"
    case "$archs" in *arm64*) ;; *) die ".app 缺 arm64 架构（lipo -archs 输出：${archs}）" ;; esac
    case "$archs" in
        *x86_64*) say "注意: 含 x86_64（意外，但放行）" ;;
        *) say "注意: 仅 arm64 —— ort-sys rc.15 无 x86_64 预编译，属预期" ;;
    esac

    # CFBundleVersion 复核：注入没生效的话 ASC 会因为 build 号重复拒收
    local got_build
    got_build=$(plist_str "$app/Contents/Info.plist" CFBundleVersion)
    [ "$got_build" = "$build_num" ] \
        || die "CFBundleVersion 注入失败（期望 ${build_num}，实际 ${got_build:-空}）"

    mkdir -p "$OUT_DIR"
    local zip="$OUT_DIR/${BUNDLE_ID}-store-${VERSION}-${build_num}.app.zip"
    rm -f "$zip"
    ditto -c -k --keepParent "$app" "$zip"
    [ -s "$zip" ] || die "zip 为空：${zip}"

    echo ""
    echo "=============================================="
    echo "  ✅ pack 完成"
    echo "=============================================="
    say "产物: ${zip}"
    say "大小: $(du -h "$zip" | cut -f1)"
    echo ""
    echo "下一步（本机）:"
    echo "  bash scripts/build-appstore.sh sign --zip ${zip}"
}

# ─────────────────────────────────────────────────────────────────────
# sign：嵌描述文件 + 沙箱 entitlements + Apple Distribution 签名 + productbuild
# ─────────────────────────────────────────────────────────────────────
find_profile_for() {
    # $1 = 期望的 bundle id（不带 TEAMID 前缀）
    # 按后缀匹配而不是精确匹配：team id 前缀无需由调用方猜出来。
    # 注意不能用 `PlistBuddy ... /dev/stdin` —— 实测报
    #   Cannot parse a NULL or zero-length data / Error Reading File: /dev/stdin
    # 必须落成真实文件再读。
    local want="$1" cand app_id probe
    for cand in "$HOME/Desktop/rocktier-certs/"*.provisionprofile \
                "$HOME/Library/MobileDevice/Provisioning Profiles/"*.provisionprofile; do
        [ -f "$cand" ] || continue
        probe=$(mktemp /tmp/rocktier-ocr-prof-probe-XXXXXX)
        if security cms -D -i "$cand" > "$probe" 2>/dev/null; then
            app_id=$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$probe" 2>/dev/null || true)
            rm -f "$probe"
            case "$app_id" in
                ?*."${want}") echo "$cand"; return 0 ;;
            esac
        else
            rm -f "$probe"
        fi
    done
    return 1
}

cmd_sign() {
    local app="$1" profile="$2"

    command -v productbuild >/dev/null 2>&1 || die "缺 productbuild（需要 Command Line Tools）"

    echo "=============================================="
    echo "  MAS sign：签名 + 出 .pkg"
    echo "=============================================="

    local bin_id
    bin_id=$(plist_str "$app/Contents/Info.plist" CFBundleIdentifier)
    [ -n "$bin_id" ] || die "读不到 .app 的 CFBundleIdentifier"

    # ── 架构 ────────────────────────────────────────────────────
    # OCR 特例：ort-sys rc.15 无 x86_64 预编译 ONNX Runtime，商店轨与直发轨
    # 一致出 arm64 单架构。hostArchitectures 会写 arm64，Intel Mac 装不上 ——
    # 这是当前上游约束下的已知取舍，不是事故。
    local bin_file archs
    bin_file=$(find "$app/Contents/MacOS" -maxdepth 1 -type f -perm -111 -print -quit)
    [ -n "$bin_file" ] || die "找不到可执行文件"
    archs=$(lipo -archs "$bin_file" 2>/dev/null || echo "")
    say "架构:     ${archs:-未知}"
    case "$archs" in
        *arm64*x86_64*|*x86_64*arm64*) say "注意: universal（意外，但放行）" ;;
        *arm64*) say "注意: 仅 arm64 —— ort-sys rc.15 无 x86_64 预编译，属预期" ;;
        *)
            if [ "$ALLOW_NONSTORE" = "1" ]; then
                echo "⚠️  非 arm64 .app（${archs}）已按 --allow-nonstore 放行 —— 该包不可提交 ASC"
            else
                die ".app 缺 arm64 架构（实际：${archs:-未知}）。OCR 商店包至少要含 arm64。"
            fi
            ;;
    esac

    # ── 分类（缺失不致命，ASC 页面上仍可手选，但两处不一致会被审核问）──
    local app_cat
    app_cat=$(plist_str "$app/Contents/Info.plist" LSApplicationCategoryType)
    if [ -z "$app_cat" ]; then
        say "分类:     未设置（ASC 里 Primary Category 请手选「生产力」）"
    else
        say "分类:     ${app_cat}"
    fi

    # ── 证书探测 ────────────────────────────────────────────────
    # 注意：不能加 -p codesigning —— Installer 证书属于 installer policy，
    # 不在该过滤结果里，会假报「0 张」（mac-signing-preflight.sh 踩过）。
    local sign_id installer_id
    if [ -n "${MAS_SIGN_ID:-}" ]; then
        sign_id="$MAS_SIGN_ID"
    else
        sign_id=$(security find-identity -v | awk -F'"' '/Apple Distribution:/ {print $2; exit}')
    fi
    [ -n "$sign_id" ] || die "钥匙串里没有 Apple Distribution 身份（跑 security find-identity -v 核对；不要加 -p codesigning）"

    if [ -n "${MAS_INSTALLER_ID:-}" ]; then
        installer_id="$MAS_INSTALLER_ID"
    else
        installer_id=$(security find-identity -v | awk -F'"' '/Installer:/ && !/Developer ID/ {print $2; exit}')
    fi
    [ -n "$installer_id" ] || die "钥匙串里没有 Installer 身份（Mac Installer Distribution / 3rd Party Mac Developer Installer）"

    say "产品:     ${APP_NAME}"
    say "Bundle:   ${bin_id}"
    say ".app 身份: ${sign_id}"
    say ".pkg 身份: ${installer_id}"

    # ── 描述文件 ────────────────────────────────────────────────
    if [ -z "$profile" ]; then
        profile=$(find_profile_for "$bin_id" || true)
    fi
    [ -n "$profile" ] && [ -f "$profile" ] \
        || die "找不到匹配 ${bin_id} 的描述文件。用 MAS_PROFILE=<路径> 或 --profile <路径> 指定"

    local prof_plist
    tmpfile /tmp/rocktier-ocr-mas-profile-XXXXXX
    prof_plist="$TMP_LAST"
    security cms -D -i "$profile" > "$prof_plist" 2>/dev/null \
        || die "无法解析描述文件：${profile}"

    local app_id team_id kcg_name expiry
    app_id=$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.application-identifier' "$prof_plist" 2>/dev/null || true)
    team_id=$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:com.apple.developer.team-identifier' "$prof_plist" 2>/dev/null || true)
    [ -n "$app_id" ] || die "描述文件里没有 application-identifier —— 拿错成开发/其他类型的描述文件了"
    [ -n "$team_id" ] || die "描述文件里没有 team-identifier"

    # 正向确认这是 Mac App Store 类型：MAS 描述文件没有 ProvisionedDevices
    if /usr/libexec/PlistBuddy -c 'Print :ProvisionedDevices' "$prof_plist" >/dev/null 2>&1; then
        die "描述文件带 ProvisionedDevices，是 development 类型，不能用于 App Store 提交"
    fi

    kcg_name=$(/usr/libexec/PlistBuddy -c 'Print :Entitlements:keychain-access-groups:0' "$prof_plist" 2>/dev/null || echo "${team_id}.*")

    # ExpirationDate 是 plist 的 <date> 类型，PlistBuddy 打印格式不可直接给 date -j 用，
    # 所以仍走 plutil；但先探一次成功与否，避免失败时错误文本被当成日期。
    local expiry=""
    if plutil -extract ExpirationDate raw -o - "$prof_plist" >/dev/null 2>&1; then
        expiry=$(plutil -extract ExpirationDate raw -o - "$prof_plist" 2>/dev/null || echo "")
    fi
    if [ -n "$expiry" ]; then
        local exp_s now_s
        exp_s=$(date -j -f "%Y-%m-%dT%H:%M:%SZ" "$expiry" "+%s" 2>/dev/null || echo 0)
        now_s=$(date "+%s")
        [ "$exp_s" -gt "$now_s" ] || die "描述文件已过期（${expiry}）"
        say "描述文件: $(basename "$profile")（有效期至 ${expiry}）"
    fi

    # bundle id 必须与描述文件里的完全一致，否则装上也起不来
    local want_id="${app_id#*.}"
    [ "$want_id" = "$bin_id" ] \
        || die "bundle id 不匹配：.app 是 ${bin_id}，描述文件是 ${want_id}"
    say "描述文件 ID 匹配 ✓"

    # ── 嵌描述文件 ──────────────────────────────────────────────
    cp "$profile" "$app/Contents/embedded.provisionprofile"
    say "已嵌入 embedded.provisionprofile"

    # ── 合并 entitlements ───────────────────────────────────────
    # 模板只写「能力」，身份三项由描述文件注入（见 Entitlements.appstore.plist 头部注释）
    local merged
    tmpfile /tmp/rocktier-ocr-mas-ent-XXXXXX
    merged="$TMP_LAST"
    cp "$SCRIPT_DIR/src-tauri/Entitlements.appstore.plist" "$merged"
    /usr/libexec/PlistBuddy -c "Add :com.apple.application-identifier string ${app_id}" "$merged"
    /usr/libexec/PlistBuddy -c "Add :com.apple.developer.team-identifier string ${team_id}" "$merged"
    /usr/libexec/PlistBuddy -c "Add :keychain-access-groups array" "$merged"
    /usr/libexec/PlistBuddy -c "Add :keychain-access-groups:0 string ${kcg_name}" "$merged"

    # ── 签名 ────────────────────────────────────────────────────
    local nested
    find "$app/Contents" -type f \( -name '*.dylib' -o -name '*.so' \) -print 2>/dev/null \
    | while IFS= read -r nested; do
        codesign --force --sign "$sign_id" "$nested"
    done

    echo ""
    echo "🔏 签名 .app（不加 --options runtime / 不加 --timestamp）..."
    codesign --force --sign "$sign_id" --entitlements "$merged" "$app"

    # ── 硬断言 ──────────────────────────────────────────────────
    codesign --verify --deep --strict "$app" || die ".app 签名自校验失败"

    local info
    info=$(codesign -dv --verbose=4 "$app" 2>&1 || true)
    case "$info" in
        *"Authority=Apple Distribution"*) ;;
        *) die ".app 不是 Apple Distribution 签名（而是：$(printf '%s' "$info" | awk -F'Authority=' '/Authority=/{print $2; exit}')）" ;;
    esac
    case "$info" in
        *"runtime)"*) die ".app 带 hardened runtime —— MAS 轨不要它（是不是用直发轨的 Entitlements.plist 签过？）" ;;
    esac

    local ents
    ents=$(codesign -d --entitlements :- "$app" 2>/dev/null || true)
    local k
    for k in com.apple.security.app-sandbox com.apple.application-identifier com.apple.developer.team-identifier; do
        case "$ents" in
            *"$k"*) ;;
            *) die "签名缺 ${k} —— ASC 会在上传阶段拒（而 codesign --verify 是绿的，所以必须显式断言）" ;;
        esac
    done
    say "entitlements 三项齐 ✓（app-sandbox / application-identifier / team-identifier）"

    # ── productbuild 出 .pkg ────────────────────────────────────
    local build_num
    build_num=$(plist_str "$app/Contents/Info.plist" CFBundleVersion)
    [ -n "$build_num" ] || build_num="$VERSION"

    mkdir -p "$OUT_DIR"
    local pkg="$OUT_DIR/${BUNDLE_ID}-${VERSION}-${build_num}.pkg"
    rm -f "$pkg"

    echo ""
    echo "📦 productbuild 出 .pkg..."
    productbuild --component "$app" /Applications --sign "$installer_id" "$pkg"
    [ -s "$pkg" ] || die "pkg 为空：${pkg}"

    local sig
    sig=$(pkgutil --check-signature "$pkg" 2>&1 || true)
    case "$sig" in
        *"3rd Party Mac Developer Installer"*|*"Mac Installer Distribution"*) ;;
        *) die ".pkg 签名不是 Installer 证书：$(printf '%s' "$sig" | head -2)" ;;
    esac

    echo ""
    echo "=============================================="
    echo "  ✅ sign 完成"
    echo "=============================================="
    say "产物: ${pkg}"
    say "大小: $(du -h "$pkg" | cut -f1)"
    say "版本: ${VERSION} (build ${build_num})"
    echo ""
    echo "下一步（都要人工做）:"
    echo "  1) Transporter.app 上传 —— CLT 16.4 已移除 xcrun altool，别再用 --upload-app"
    echo "  2) ASC 的 Primary Category 选「生产力」（对应 tauri.appstore.conf.json 的 Productivity）"
    echo "  3) 付费应用协议（Paid Applications）必须已生效，否则构建卡在「等待协议」"
    echo "  4) 在 ASC 里确认构建版本到达后，绑定到版本页再提交审核"
}

# ─────────────────────────────────────────────────────────────────────
# 入口
# ─────────────────────────────────────────────────────────────────────
usage() {
    sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

[ $# -ge 1 ] || usage 1
cmd="$1"; shift

APP_PATH=""
ZIP_PATH=""
PROFILE_PATH="${MAS_PROFILE:-}"
BUILD_NUM_ARG=""
ALLOW_NONSTORE=0

while [ $# -gt 0 ]; do
    case "$1" in
        --app)     APP_PATH="${2:-}"; shift 2 ;;
        --zip)     ZIP_PATH="${2:-}"; shift 2 ;;
        --profile) PROFILE_PATH="${2:-}"; shift 2 ;;
        --build-num) BUILD_NUM_ARG="${2:-}"; shift 2 ;;
        --allow-nonstore) ALLOW_NONSTORE=1; shift ;;
        -h|--help) usage 0 ;;
        *) die "未知参数：$1" ;;
    esac
done

case "$cmd" in
    pack)
        build_num="${BUILD_NUM_ARG:-${BUILD_NUM:-}}"
        if [ -z "$build_num" ]; then
            build_num=$(git rev-list --count HEAD 2>/dev/null || date +%Y%m%d%H%M)
        fi
        case "$build_num" in
            ''|*[!0-9]*) die "build 号必须是纯数字（当前：${build_num}）" ;;
        esac
        cmd_pack "$build_num"
        ;;
    sign)
        if [ -n "$ZIP_PATH" ]; then
            [ -f "$ZIP_PATH" ] || die "找不到 zip：${ZIP_PATH}"
            # 渠道守卫：CI 产物名带 -store-，直发包重签进商店会被 3.1.1 拒
            case "$(basename "$ZIP_PATH")" in
                *-store-*) ;;
                *)
                    if [ "$ALLOW_NONSTORE" != "1" ]; then
                        die "这个 zip 不是 store 渠道产物（文件名缺 -store-）：$(basename "$ZIP_PATH")
   直发渠道的 .app 带自研付费墙，进 App Store 会被审核指南 3.1.1 拒。
   确认无误可加 --allow-nonstore 强制继续（仅用于联调签名链路）。"
                    fi
                    echo "⚠️  已按 --allow-nonstore 放行：签名后的包**不可提交 ASC**（内容仍是直发渠道）"
                    ;;
            esac
            tmpdir /tmp/rocktier-ocr-mas-app-XXXXXX
            extract_dir="$TMP_LAST"
            ditto -x -k "$ZIP_PATH" "$extract_dir"
            APP_PATH=$(find "$extract_dir" -maxdepth 1 -name '*.app' -print -quit 2>/dev/null || true)
        fi
        [ -n "$APP_PATH" ] && [ -d "$APP_PATH" ] \
            || die "需要 --zip <store 产物 zip> 或 --app <已有 .app 路径>"
        cmd_sign "$APP_PATH" "$PROFILE_PATH"
        ;;
    -h|--help|help) usage 0 ;;
    *) usage 1 ;;
esac
