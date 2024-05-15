#!/bin/bash

# 定义下载的URL和安装目标目录
URL="https://github.com/huahuadeliaoliao/biCat/releases/download/v0.1.0/bicat"
DESTINATION="$HOME/.local/bin/bicat"

# 确保目标安装目录存在
mkdir -p $HOME/.local/bin

# 使用curl下载bicat二进制文件
echo "Downloading biCat from $URL..."
curl -L $URL -o $DESTINATION

# 给予文件执行权限
echo "Setting execution permissions for biCat..."
chmod +x $DESTINATION

# 检查bicat是否成功安装
if [ -f $DESTINATION ]; then
    echo "biCat installed successfully. You can now use it by typing 'bicat'."
    echo "Remember to add $HOME/.local/bin to your PATH if it's not already done."
    echo "You can add the following line to your .bashrc or .zshrc:"
    echo 'export PATH="$HOME/.local/bin:$PATH"'
else
    echo "Failed to install biCat. Please check the errors above."
fi
