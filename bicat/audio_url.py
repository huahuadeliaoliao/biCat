import requests
import re
import json

# 伪造请求头
headers = {
    "Connection": "Keep-Alive",
    "Accept-Language": "en-US,en;q=0.8,zh-Hans-CN;q=0.5,zh-Hans;q=0.3",
    "Accept": "text/html, application/xhtml+xml, */*",
    "referer": "https://www.bilibili.com",
    "user-agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:99.0) Gecko/20100101 Firefox/99.0",
}


# 获取响应内容
def get_page(url):
    try:
        s = requests.get(url, headers=headers)
        s.raise_for_status()
        s.encoding = s.apparent_encoding
        return s
    except Exception as e:
        return f"解析网页失败，请检查！错误信息: {e}"


# 解析页面获取音频 URL
def parse_page(data):
    json_str = re.findall("<script>window.__playinfo__=(.*?)</script>", data, re.S)[0]
    json_data = json.loads(json_str)
    audio_url = json_data["data"]["dash"]["audio"][0]["baseUrl"]  # 音频 URL
    return audio_url


# 写入音频文件
def write_res(filename, data, path):
    with open(f"{path}/{filename}", "wb") as f:
        f.write(data)


# 通过 BV 号获取下载音频
def getAudioByBV(bvNo, path):
    url = f"http://www.bilibili.com/video/{bvNo}"
    html_data = get_page(url).text
    audio_url = parse_page(html_data)
    audio_content = get_page(audio_url).content
    write_res(filename=bvNo + ".mp3", data=audio_content, path=path)
    print("保存音频成功！")


if __name__ == "__main__":
    bvNo = "BV1JN4y1v7Xw"
    path = "/home/huahua/python"
    getAudioByBV(bvNo, path)
