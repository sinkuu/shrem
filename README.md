# shrem
rm-like wrapper for `shred`.

```bash
$ shrem -rv dir
shred: dir/file: pass 1/4 (random)...
shred: dir/file: pass 2/4 (random)...
shred: dir/file: pass 3/4 (random)...
shred: dir/file: pass 4/4 (000000)...
shred: dir/file: removing
shred: dir/file: renamed to dir/0000
shred: dir/0000: renamed to dir/000
shred: dir/000: renamed to dir/00
shred: dir/00: renamed to dir/0
shred: dir/file: removed
shrem: dir: removing
shrem: dir: renamed to 000
shrem: 000: renamed to 00
shrem: 00: renamed to 0
shrem: 0: removed
```
