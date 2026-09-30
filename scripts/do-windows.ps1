# do-windows.ps1 — ĐO những gì huba cần biết trước khi nối phiên với cửa sổ trên Windows.
#
# Chạy trên máy Windows (PowerShell thường, KHÔNG "Run as Administrator"):
#   powershell -ExecutionPolicy Bypass -File scripts\do-windows.ps1
# Kết quả: một tệp văn bản ở %TEMP%\huba-do-windows-<mốc>.txt — gửi tệp ấy về.
#
# Vì sao có tệp này (Hà 30/09 «viết để chạy được window»): huba trên macOS nối mỗi phiên
# `claude` với cửa sổ của nó qua bảng `ps` (pid → tty). Windows không có tty, và bản
# `keys_win.rs` (08/09) được viết mà CHƯA từng chạy trên máy Windows nào. Viết phần nối
# phiên bằng đoán là vá mù; tệp này thu đúng các dữ kiện còn thiếu để viết bằng số đo:
#   ① `claude agents --json` trên Windows in ra gì (có pid không, có tty không)
#   ② `claude` chạy dưới tiến trình nào (claude.exe? node.exe cli.js?) và cha của nó là ai
#   ③ Windows Terminal có đúng lớp cửa sổ CASCADIA_HOSTING_WINDOW_CLASS không, pid của nó
#   ④ Đọc bảng tiến trình mất bao lâu (Win32_Process) — huba đọc nó mỗi ảnh chụp
#   ⑤ HOME / USERPROFILE / ~\.claude có ở đâu
#
# CHỈ ĐỌC: không cài gì, không sửa gì, không in giá trị bí mật (chỉ in CÓ/KHÔNG của khoá).
# ⚠ Dòng lệnh của tiến trình có thể chứa đề bài của phiên — đọc lại tệp trước khi gửi.

$ErrorActionPreference = 'Continue'
$moc = Get-Date -Format 'yyyyMMdd-HHmmss'
$out = Join-Path $env:TEMP "huba-do-windows-$moc.txt"

function Muc($ten) { "`r`n===== $ten =====" | Out-File -FilePath $out -Append -Encoding utf8 }
function Ghi($x) { ($x | Out-String -Width 400) | Out-File -FilePath $out -Append -Encoding utf8 }

"huba do-windows $moc" | Out-File -FilePath $out -Encoding utf8

Muc 'MAY'
Ghi ([Environment]::OSVersion.VersionString)
Ghi ("PowerShell " + $PSVersionTable.PSVersion)
Ghi (Get-AppxPackage -Name Microsoft.WindowsTerminal -ErrorAction SilentlyContinue | Select-Object Name, Version)
Ghi ("wt.exe: " + ((Get-Command wt.exe -ErrorAction SilentlyContinue).Source))
Ghi ("dang nang quyen (Administrator): " + ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))

Muc 'THU MUC NHA'
Ghi ("HOME=" + $env:HOME)
Ghi ("USERPROFILE=" + $env:USERPROFILE)
Ghi ("LOCALAPPDATA=" + $env:LOCALAPPDATA)
$claudeDir = Join-Path $env:USERPROFILE '.claude'
Ghi ("~\.claude co: " + (Test-Path $claudeDir))
Ghi ("~\.claude\projects co: " + (Test-Path (Join-Path $claudeDir 'projects')))
$book = Join-Path $env:USERPROFILE '.claude.json'
Ghi ("~\.claude.json co: " + (Test-Path $book))
if (Test-Path $book) {
    try {
        $j = Get-Content $book -Raw | ConvertFrom-Json
        Ghi ("  oauthAccount co: " + ($null -ne $j.oauthAccount))
        Ghi ("  cachedUsageUtilization co: " + ($null -ne $j.cachedUsageUtilization))
    } catch { Ghi ("  KHONG DOC DUOC JSON: " + $_.Exception.Message) }
}

Muc 'CLAUDE CLI'
$cl = Get-Command claude -ErrorAction SilentlyContinue
Ghi ("claude: " + $cl.Source + "  (" + $cl.CommandType + ")")
Ghi (& claude --version 2>&1)
Muc 'claude agents --json (NGUYEN VAN)'
$t = Measure-Command { $agents = & claude agents --json 2>&1 }
Ghi ("mat " + [int]$t.TotalMilliseconds + " ms")
Ghi $agents

Muc 'TIEN TRINH (claude/node/terminal/shell) — pid, cha, ten, dong lenh'
$t = Measure-Command { $procs = Get-CimInstance Win32_Process }
Ghi ("doc Win32_Process: " + [int]$t.TotalMilliseconds + " ms, " + $procs.Count + " tien trinh")
Ghi ($procs | Where-Object { $_.Name -match 'claude|node|WindowsTerminal|OpenConsole|conhost|powershell|pwsh|cmd\.exe|bash' } |
     Select-Object ProcessId, ParentProcessId, Name, CommandLine | Format-Table -AutoSize -Wrap)

Muc 'CUA SO CAP CAO (lop, tieu de, pid)'
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class HubaDo {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    public static List<string> All() {
        var r = new List<string>();
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            var c = new StringBuilder(256); GetClassName(h, c, 256);
            var t = new StringBuilder(512); GetWindowText(h, t, 512);
            uint pid; GetWindowThreadProcessId(h, out pid);
            r.Add(h.ToInt64() + "`t" + pid + "`t" + c + "`t" + t);
            return true;
        }, IntPtr.Zero);
        return r;
    }
}
"@
Ghi "hwnd`tpid`tlop`ttieu de"
Ghi ([HubaDo]::All())

Muc 'XONG'
Ghi ("tep: " + $out)
Write-Host "Da ghi: $out"
