# shrem
rm-like wrapper for `shred`.

```bash
$ shrem -rv dir
shred: dir/BB: removing
shred: dir/BB: renamed to dir/00
shred: dir/00: renamed to dir/0
shred: dir/BB: removed
shred: dir/AA: removing
shred: dir/AA: renamed to dir/00
shred: dir/00: renamed to dir/0
shred: dir/AA: removed
shrem: dir: removing
shrem: dir: renamed to 000
shrem: 000: renamed to 00
shrem: 00: renamed to 0
shrem: 0: removed
```
