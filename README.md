jbodncp
=============
Massive parallel JBOD-aware file transfer

## Synopsis
Start the listener on your source machine:
```
src$ jbodncp serve /pool/storage/
[INFO] Bearer token for this session: 057g3vM9uqEsJn5iJ81NPQPap2diIaOu
[INFO] listening on 0.0.0.0:3000
```
Run the client on the opposite end:
```
dst$ jbodncp download --auth 057g3vM9uqEsJn5iJ81NPQPap2diIaOu  --threads 16 http://src-ip:3000 /pool/storage/
root@v351102:~/paraflow# ls -lh /tmp/3/
[INFO] Downloading URL: http://src-ip:3000/download/00000/eroa9ahmzgp0 => /pool/storage/00000/eroa9ahmzgp0
[INFO] Downloading URL: http://src-ip:3000/download/00000/1kr3rsxht9u1 => /pool/storage/00000/1kr3rsxht9u1
[INFO] Downloading URL: http://src-ip:3000/download/00000/7ebrxgmvclfs => /pool/storage/00000/7ebrxgmvclfs
```
Now all you have to do is to wait until the transfer completes. There are some points to keep in mind:

* Missing directory paths are going to be automatically created
* Existing files are not going to be overwritten unless file size differs
* We use a plain HTTP connection by default. If you're concerned about that, put **jbodncp serve** under an HTTPS reverse proxy such as Nginx.

## Getting harder
Now suppose you have a server with the files spread across 4 different mount points. In this case, **jbodncp** can also do the job for you.
Just specify multiple directory paths as command line arguments and see what happens:
````
# Hint: use shell completions to avoid typing manually
src$ jbodncp serve /pool/storage01/ /pool/storage02/ /pool/storage03/ /pool/storage04/
````
Imagine that we only have 2 storage mount points on the destination server. Of course, that's not a problem either:
````
dst$ jbodncp download <...> http://src-ip:3000 /pool/storage01/ /pool/storage02/
[INFO] Downloading URL: http://src-ip:3000/download/00000/eroa9ahmzgp0 => /pool/storage01/00000/eroa9ahmzgp0
[INFO] Downloading URL: http://src-ip:3000/download/00000/1kr3rsxht9u1 => /pool/storage02/00000/1kr3rsxht9u1
[INFO] Downloading URL: http://src-ip:3000/download/00000/7ebrxgmvclfs => /pool/storage01/00000/7ebrxgmvclfs
...
````
**jbodncp** obeys the following rules when working in JBOD mode to mitigate the stopped transfer artifacts problem:
* If there are two or more files with the same relative path in different source locations, the one with maximal file size is getting served
* Each time when downloading a file, we check if a file with the same relative path already exists in one of destination locations. So, we rewrite an already existing one rather than creating a new copy in another location (or do nothing if it's file size is equal to the orig)
* In all another cases, we use the round robin principle to select a destination for each incoming file
