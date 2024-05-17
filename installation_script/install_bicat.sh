#!/bin/bash

# 定义仓库的基础URL和安装目标目录
REPO_URL="https://github.com/huahuadeliaoliao/biCat"
RELEASE_URL="https://github.com/huahuadeliaoliao/biCat/releases/download/v0.1.0"
DESTINATION="$HOME/.local/bin/bicat"

# 确保目标安装目录存在
mkdir -p $HOME/.local/bin

# 检查操作系统和架构
OS=$(uname -s)
ARCH=$(uname -m)

if [[ "$OS" == "Linux" ]]; then
    URL="$RELEASE_URL/bicat"
elif [[ "$OS" == "Darwin" ]]; then
    if [[ "$ARCH" == "arm64" ]]; then
        URL="$REPO_URL/raw/main/MacOS/aarch64/bicat"
    else
        URL="$REPO_URL/raw/main/MacOS/x86/bicat"
    fi
else
    echo "不支持的操作系统：$OS"
    exit 1
fi

# 使用curl下载bicat二进制文件
echo "从 $URL 下载 biCat..."
curl -L $URL -o $DESTINATION

# 给予文件执行权限
echo "为 biCat 设置执行权限..."
chmod +x $DESTINATION

# 检查bicat是否成功安装
if [ -f $DESTINATION ]; then
    echo "biCat 安装成功。你现在可以通过输入 'bicat' 来使用它了。"
    echo "如果 $HOME/.local/bin 不在你的 PATH 中，请记得添加。"
    echo "你可以在 .bashrc 或 .zshrc 中添加以下行："
    echo 'export PATH="$HOME/.local/bin:$PATH"'
else
    echo "安装 biCat 失败。请检查上述错误。"
fi
