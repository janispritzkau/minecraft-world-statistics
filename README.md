# Minecraft World Statistics

## Count items in a world

> not fully implemented yet

```sh
dump-items world/ \
  --entities chest_minecart,item_frame,glow_item_frame \
  --block-entities chest,shulker_box,barrel \
  overworld:r=8000 nether:r=1000 end:r=1000 playerdata:inventory,ender_chest > items.txt

count-items < items.txt > total-items.json
```
