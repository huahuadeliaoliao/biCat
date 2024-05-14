中文版 | [English](README_en.md)

<p align="center">
    <img src="img/orange-cat-sleep.gif" width="200" height="200">
</p>
<h1 align="center">biCat</h1>

# 简介
一个速度极快的下载 bilibili 视频的音频文件的命令行工具，基于tokio，无须登录（目标收藏夹需要在下载的时候处于公开状态）

## 版本
Beta 0.1.0(从收藏夹下载音频的功能已经实现，可以用来直接使用，但是目前的错误信息十分简陋，功能也仅实现了bicat [media_id]进行下载，所以暂时只打包了deb)

### 主要功能
- **下载收藏夹视频的音频文件**：从 Bilibili 的指定收藏夹 media_id（也就是web端收藏夹网址fid后面的数字）下载所有视频的音频文件到当前目录下，保存为视频标题-视频作者名字.mp3。
- **进度显示**：显示下载进度条，包括已完成的任务数和预估剩余时间。
- **示例**：
  
  <img src="img/beta_example.png" width="500" height="100" alt="orangecat">

# 使用说明
工具仅用于个人学习使用，请勿用于任何商业用途，下载的内容请及时删除，尊重原作者版权