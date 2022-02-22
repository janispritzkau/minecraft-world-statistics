# Minecraft World Statistics

## Count items in a world

```sh
dump-items world/ \
  --entities chest_minecart,item_frame,glow_item_frame \
  --block-entities chest,shulker_box,barrel \
  overworld:chunk_radius=512 \
  nether:chunk_radius=128 \
  end:chunk_radius=128 \
> items.txt

count-items < items.txt > total-items.json
```
