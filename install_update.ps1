# install_update.ps1 — bản Windows của install_update.sh: dựng → cài → chạy nền tự khởi động.
#
#   powershell -ExecutionPolicy Bypass -File install_update.ps1            # dựng từ mã rồi cài
#   powershell -ExecutionPolicy Bypass -File install_update.ps1 -BinDir D:\huba-win   # cài bản dựng sẵn
#   powershell -ExecutionPolicy Bypass -File install_update.ps1 -Verify    # CHỈ ĐỌC: bản cài có phải bản vừa dựng
#
# Chạy bằng PowerShell THƯỜNG — KHÔNG "Run as Administrator". huba gõ phím (SendInput) và đọc
# màn (UI Automation) vào cửa sổ Windows Terminal; Windows CHẶN hai việc ấy giữa hai mức quyền
# khác nhau (UIPI), nên huba phải chạy CÙNG mức quyền với các cửa sổ `claude` — tức mức thường.
#
# Khác macOS ở ba chỗ (Hà 30/09 «viết để chạy được window»):
#   · không ký chứng chỉ: Windows không gắn quyền theo chữ ký như TCC (xem rust/src/keys_win.rs);
#   · launchd ⟹ Task Scheduler, tác vụ `huba-hubd`, chạy lúc đăng nhập, tự chạy lại khi chết;
#   · cài ra %LOCALAPPDATA%\hub\bin (hubd.exe + huba.exe), thư mục ấy vào PATH người dùng,
#     và HUB_CONFIG trỏ vào huba.config.json của cây này để `huba` gõ ở đâu cũng tìm ra cấu hình.
#
# ⚠ CHƯA CHẠY THỬ TRÊN MÁY WINDOWS THẬT (viết trên macOS, 30/09). Mỗi bước in mốc và DỪNG khi hỏng.
# ⚠ Một bot Telegram chỉ nhận lệnh ở MỘT nơi: hubd trên Windows mà dùng CÙNG bot với máy Mac thì
#   hai bên giành getUpdates (409 Conflict). Dùng bot riêng cho máy Windows, hoặc tắt hubd bên kia.

param(
    [string]$BinDir = '',
    [switch]$Verify
)
$ErrorActionPreference = 'Stop'

$Here   = Split-Path -Parent $MyInvocation.MyCommand.Path
$Rust   = Join-Path $Here 'rust'
$Dest   = Join-Path $env:LOCALAPPDATA 'hub\bin'
$Task   = 'huba-hubd'
$Config = Join-Path $Here 'huba.config.json'

function Moc($s) { Write-Host ("[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $s) }

# Nguồn của hai binary: bản dựng sẵn (-BinDir) hay dựng từ mã.
if ($BinDir) {
    $SrcHubd = Join-Path $BinDir 'hubad.exe'
    $SrcCli  = Join-Path $BinDir 'huba.exe'
} else {
    $SrcHubd = Join-Path $Rust 'target\release\hubad.exe'
    $SrcCli  = Join-Path $Rust 'target\release\huba.exe'
}
$DstHubd = Join-Path $Dest 'hubd.exe'
$DstCli  = Join-Path $Dest 'huba.exe'

if ($Verify) {
    if (-not (Test-Path $DstHubd)) { Moc "CHUA CAI: khong co $DstHubd"; exit 1 }
    if (-not (Test-Path $SrcHubd)) { Moc "KHONG DO DUOC: khong co ban dung $SrcHubd"; exit 2 }
    $a = (Get-FileHash $DstHubd -Algorithm SHA256).Hash
    $b = (Get-FileHash $SrcHubd -Algorithm SHA256).Hash
    if ($a -eq $b) { Moc "KHOP — ban cai la ban dung hien co ($a)"; exit 0 }
    Moc "LECH — ban cai $a, ban dung $b"; exit 1
}

if (-not (Test-Path $Config)) { throw "khong thay $Config — can cau hinh truoc (huba.config.json)" }

# ① Dựng (nếu không dùng bản dựng sẵn).
if (-not $BinDir) {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "khong thay cargo — cai Rust (https://rustup.rs) hoac chay voi -BinDir <thu muc co hubad.exe + huba.exe>"
    }
    Moc 'dung ban release...'
    Push-Location $Rust
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build --release thoat $LASTEXITCODE — ban dang chay GIU NGUYEN" }
    } finally { Pop-Location }
}
foreach ($f in @($SrcHubd, $SrcCli)) { if (-not (Test-Path $f)) { throw "khong thay $f" } }

# ② Dừng bản đang chạy — hubd.exe đang chạy thì Windows KHOÁ tệp, chép đè sẽ hỏng.
if (Get-ScheduledTask -TaskName $Task -ErrorAction SilentlyContinue) {
    Moc "dung tac vu $Task..."
    Stop-ScheduledTask -TaskName $Task -ErrorAction SilentlyContinue
}
$han = (Get-Date).AddSeconds(20)
while ((Get-Process -Name hubd -ErrorAction SilentlyContinue) -and ((Get-Date) -lt $han)) { Start-Sleep -Milliseconds 300 }
if (Get-Process -Name hubd -ErrorAction SilentlyContinue) {
    throw "hubd.exe van song sau 20 giay — khong chep de (ban dang chay GIU NGUYEN)"
}

# ③ Cài.
New-Item -ItemType Directory -Force -Path $Dest | Out-Null
Copy-Item $SrcHubd $DstHubd -Force
Copy-Item $SrcCli  $DstCli  -Force
Moc "da cai $DstHubd + $DstCli"

# ④ PATH + HUB_CONFIG cho người dùng (không đụng biến của máy).
$p = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (($p -split ';') -contains $Dest)) {
    [Environment]::SetEnvironmentVariable('Path', (($p.TrimEnd(';') + ';' + $Dest).TrimStart(';')), 'User')
    Moc "da them $Dest vao PATH nguoi dung (mo cua so moi moi thay)"
}
[Environment]::SetEnvironmentVariable('HUB_CONFIG', $Config, 'User')

# ⑤ Tác vụ tự khởi động: lúc đăng nhập, mức quyền THƯỜNG, chết thì chạy lại, không giới hạn giờ.
#    Thư mục làm việc = cây huba ⟹ hubd tìm ra huba.config.json + huba.env ở đây.
$action    = New-ScheduledTaskAction -Execute $DstHubd -WorkingDirectory $Here
$trigger   = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings  = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) `
                -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
                -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew
$principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $Task -Action $action -Trigger $trigger -Settings $settings `
    -Principal $principal -Force | Out-Null
Moc "da dang ky tac vu $Task"

# ⑥ Chạy và ĐO lại — mã thoát của Start-ScheduledTask chỉ nói "đã bảo chạy", không nói "đang chạy".
Start-ScheduledTask -TaskName $Task
$han = (Get-Date).AddSeconds(15)
$proc = $null
while (-not $proc -and ((Get-Date) -lt $han)) {
    Start-Sleep -Milliseconds 500
    $proc = Get-Process -Name hubd -ErrorAction SilentlyContinue
}
if (-not $proc) {
    throw "hubd.exe KHONG song sau 15 giay — xem logs\huba.log va 'Get-ScheduledTaskInfo $Task'"
}
Moc ("hubd dang chay, pid " + ($proc | Select-Object -First 1).Id)
