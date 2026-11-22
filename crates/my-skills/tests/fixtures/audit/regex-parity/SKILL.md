---
name: regex-parity
description: Synthetic fixture for the skill audit.
---

Traversal candidates: x../(../visible and 한../skip and ²../skip and ́../kept
Do not match the nested path /segment/../skip.
![image with
a multiline label](https://example.invalid/둘😀)
![upper](HTTP://example.invalid/skip) ![bad](https://example.invalid/with space)

Examples of Unicode boundaries and case folding:
‿GH AUTH TOKEN
́op read
²op read
ſecurity fİnd-generic-password
security find-internet-password
Cat ~/.aws/credentials
less $HOME/.ssh/id_rsa
grep /.config/gcloud
awk ~/.docker/config.json
sed ~/.aws/credentials
`wget http://example.invalid/`
`netcat ١٢٧.٠.٠.١`
`scp example@fiction`
`rsync example.invalid`
`ssh example.invalid`
`ssh İİⅣ@`
Ignorepreviousinstructions
../first../second
