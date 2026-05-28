# Windows 탐색기 통합 — .qs / .iso 파일 association + 우클릭 메뉴
#
# 사용 (PowerShell admin):
#   .\install-windows.ps1                # 시스템 전역 (HKLM)
#   .\install-windows.ps1 -User          # 현재 사용자만 (HKCU, sudo 불필요)
#   .\install-windows.ps1 -Uninstall     # 제거
#
# 무엇을 등록하는가:
#   ProgID:   qsafe.qsfile        — .qs 확장자
#   ProgID:   qsafe.discimage     — .iso/.img 확장자 (선택)
#   우클릭 메뉴:
#     "Compress with qsafe"       — 폴더/파일 우클릭
#     "Unpack with qsafe"          — .qs 우클릭
#
# 권장: tauri-bundler가 만든 MSI/NSIS 인스톨러를 사용하면 이 작업이 자동.

param(
    [switch]$User,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"

$Hive = if ($User) { "HKCU" } else { "HKLM" }
if (-not $User -and -not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error "전역 설치는 관리자 권한 필요 (또는 -User 옵션 사용)"
    exit 1
}

$ProgID_qs   = "qsafe.qsfile"
$ProgID_iso  = "qsafe.discimage"

# 설치된 qsafe-gui.exe 경로 찾기
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Candidates = @(
    "$ScriptDir\..\..\..\target\release\qsafe-gui.exe",
    "$ScriptDir\..\..\..\target\x86_64-pc-windows-msvc\release\qsafe-gui.exe",
    "$env:LOCALAPPDATA\Programs\qsafe\qsafe-gui.exe",
    "$env:ProgramFiles\qsafe\qsafe-gui.exe"
)
$GuiPath = $null
foreach ($c in $Candidates) {
    if (Test-Path $c) { $GuiPath = (Resolve-Path $c).Path; break }
}

if (-not $Uninstall -and -not $GuiPath) {
    Write-Error "qsafe-gui.exe를 찾을 수 없습니다. 먼저 'cargo build --release -p qsafe-gui' 실행."
    exit 1
}

function Remove-Reg($path) {
    if (Test-Path "Registry::$Hive\$path") {
        Remove-Item -Path "Registry::$Hive\$path" -Recurse -Force -ErrorAction SilentlyContinue
    }
}

if ($Uninstall) {
    Write-Host "qsafe Windows 탐색기 통합 제거 중…"
    Remove-Reg "Software\Classes\.qs"
    Remove-Reg "Software\Classes\$ProgID_qs"
    Remove-Reg "Software\Classes\$ProgID_iso"
    Remove-Reg "Software\Classes\*\shell\qsafe-compress"
    Remove-Reg "Software\Classes\Directory\shell\qsafe-compress"
    Write-Host "제거 완료."
    exit 0
}

Write-Host "qsafe Windows 탐색기 통합 ($Hive) …"
Write-Host "  qsafe-gui: $GuiPath"

# 1) .qs 확장자 → ProgID 매핑
New-Item -Path "Registry::$Hive\Software\Classes\.qs" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\.qs" -Name "(Default)" -Value $ProgID_qs

# 2) ProgID 정의 (이름, 아이콘, open 명령)
New-Item -Path "Registry::$Hive\Software\Classes\$ProgID_qs" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\$ProgID_qs" -Name "(Default)" -Value "qsafe Archive"

# 아이콘 (qsafe-gui.exe 첫 아이콘 사용)
New-Item -Path "Registry::$Hive\Software\Classes\$ProgID_qs\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\$ProgID_qs\DefaultIcon" -Name "(Default)" -Value "`"$GuiPath`",0"

# open 명령 (더블 클릭)
New-Item -Path "Registry::$Hive\Software\Classes\$ProgID_qs\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\$ProgID_qs\shell\open\command" -Name "(Default)" -Value "`"$GuiPath`" `"%1`""

Write-Host "  ✓ .qs → qsafe-gui 더블 클릭"

# 3) 우클릭 메뉴 — 모든 파일/폴더에 "Compress with qsafe"
New-Item -Path "Registry::$Hive\Software\Classes\*\shell\qsafe-compress" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\*\shell\qsafe-compress" -Name "(Default)" -Value "Compress with qsafe"
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\*\shell\qsafe-compress" -Name "Icon" -Value "`"$GuiPath`",0"
New-Item -Path "Registry::$Hive\Software\Classes\*\shell\qsafe-compress\command" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\*\shell\qsafe-compress\command" -Name "(Default)" -Value "`"$GuiPath`" --action=pack `"%1`""

New-Item -Path "Registry::$Hive\Software\Classes\Directory\shell\qsafe-compress" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\Directory\shell\qsafe-compress" -Name "(Default)" -Value "Compress with qsafe"
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\Directory\shell\qsafe-compress" -Name "Icon" -Value "`"$GuiPath`",0"
New-Item -Path "Registry::$Hive\Software\Classes\Directory\shell\qsafe-compress\command" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\Directory\shell\qsafe-compress\command" -Name "(Default)" -Value "`"$GuiPath`" --action=pack `"%1`""

Write-Host "  ✓ 우클릭 → Compress with qsafe (파일 + 폴더)"

# 4) .qs 우클릭에 "Unpack with qsafe"
New-Item -Path "Registry::$Hive\Software\Classes\$ProgID_qs\shell\unpack" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\$ProgID_qs\shell\unpack" -Name "(Default)" -Value "Unpack with qsafe"
New-Item -Path "Registry::$Hive\Software\Classes\$ProgID_qs\shell\unpack\command" -Force | Out-Null
Set-ItemProperty -Path "Registry::$Hive\Software\Classes\$ProgID_qs\shell\unpack\command" -Name "(Default)" -Value "`"$GuiPath`" --action=unpack `"%1`""

Write-Host "  ✓ .qs 우클릭 → Unpack with qsafe"

Write-Host ""
Write-Host "설치 완료. 탐색기 새로고침 후 .qs 더블 클릭 / 우클릭 동작 확인."
