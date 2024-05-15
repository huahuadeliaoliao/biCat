$ErrorActionPreference = "Stop"

$BINARY_NAME = "bicat"
$URL = "https://github.com/huahuadeliaoliao/biCat/releases/download/v0.1.0/bicat.exe"
$TMP_DIR = "$env:TEMP"
$INSTALL_DIR = "$env:LocalAppData\Programs"
$TMP_FILE = "$TMP_DIR\$BINARY_NAME.exe"

Write-Output "Downloading $BINARY_NAME from $URL"
Invoke-WebRequest -Uri $URL -OutFile $TMP_FILE

if (-Not (Test-Path -Path $INSTALL_DIR)) {
    New-Item -ItemType Directory -Path $INSTALL_DIR
}

Move-Item -Path $TMP_FILE -Destination "$INSTALL_DIR\$BINARY_NAME.exe"

# 检查并添加到 PATH 环境变量
$path = [System.Environment]::GetEnvironmentVariable("Path", [System.EnvironmentVariableTarget]::User)
if ($path -split ';' -notcontains $INSTALL_DIR) {
    $newPath = "$path;$INSTALL_DIR"
    [System.Environment]::SetEnvironmentVariable("Path", $newPath, [System.EnvironmentVariableTarget]::User)
    Write-Output "$INSTALL_DIR added to PATH"
} else {
    Write-Output "$INSTALL_DIR is already in the PATH"
}

Write-Output "$BINARY_NAME installed to $INSTALL_DIR"
Write-Output "Installation complete. Please restart your terminal or log out and back in for changes to take effect."
