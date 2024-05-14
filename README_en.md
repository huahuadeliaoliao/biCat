# Introduction
A very fast command line tool to download audio files from bilibili videos, based on tokio, no login required (target favorites need to be public at the time of download).

## Version
Beta 0.1.0 (The function of downloading audio from bookmarks has been implemented and can be used directly, but the current error messages are very rudimentary, and the functionality is only achieved through the command "bicat [media_id]" for download. Therefore, only deb and exe packages are provided for now. On Windows, when using bicat in cmd, you need to add the directory where bicat.exe is located to the environment variables.)

### Main functions
- **Download audio files of favorite videos**: download audio files of all videos from Bilibili's specified favorite media_id (that is, the number after the fid of the web favorite URL) to the current directory, and save them as video title-video author name.mp3.
- **Progress Display**: displays a progress bar for the download, including the number of tasks completed and the estimated time remaining.
- **Example**:

  <img src="img/beta_example.png" width="500" height="100" alt="orangecat">

# Instructions for use
The tool is only for personal learning use, please do not use for any commercial purposes, download the content please delete in a timely manner, respect the copyright of the original author!