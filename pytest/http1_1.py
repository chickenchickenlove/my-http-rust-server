import requests
import logging, http.client
http.client.HTTPConnection.debuglevel = 0  # 필요하면 1
logging.basicConfig(level=logging.DEBUG)
logging.getLogger("urllib3").setLevel(logging.DEBUG)
logging.getLogger("urllib3.connectionpool").setLevel(logging.DEBUG)

url = "http://localhost:8080/hello"

with requests.Session() as s:
    s.headers.update({"Connection": "keep-alive"})  # 명시(사실 HTTP/1.1 기본값)
    for i in range(5):
        r = s.get(url, timeout=5)
        content = r.content  # 본문을 읽어 커넥션 풀로 반환되게 함
        print(f'ASh {content = }')

with requests.Session() as s:
    s.headers.update({"Connection": "close"})  # 명시(사실 HTTP/1.1 기본값)
    for i in range(5):
        r = s.get(url, timeout=5)
        content = r.content  # 본문을 읽어 커넥션 풀로 반환되게 함
        print(f'ASh {content = }')