


// POST /api/v1/widgets?limit=10 HTTP/1.1
// Host: example.com
// User-Agent: telnet
// Accept: application/json
// Content-Type: application/json
// Content-Length: 29
// Connection: close
//
// {"name":"foo","enabled":true}


### HTTP/1.1 Keep Alive Test
```shell

$ curl 


```


### HTTP/2
```shell
$ brew install h2spec
$ cd http_h2
$ python main.py
...

$ h2spec -p 8080 -h localhost -P /hello
$ h2spec -p 8080 -h localhost -P /hello generic/1  
```