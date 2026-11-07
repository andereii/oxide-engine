package funkin.data.song;

import flixel.util.FlxSort;
import funkin.data.song.SongData.SongTimeChange;

/**
 * Probe stub for funkin.data.song.SongDataUtils (same module-shadowing rule).
 * `sortTimeChanges` is copied verbatim from
 * source/funkin/data/song/SongDataUtils.hx:L266-274 and stays executable: it
 * sorts with the real installed FlxSort against the probe SongTimeChange
 * shape. Unrelated clipboard/serialization helpers are dropped.
 */
@:nullSafety
class SongDataUtils
{
  /** Copy of SongDataUtils.hx:L266-274 (sortTimeChanges). */
  public static function sortTimeChanges(timeChanges:Array<SongTimeChange>, desc:Bool = false):Array<SongTimeChange>
  {
    timeChanges.sort(function(a:SongTimeChange, b:SongTimeChange):Int
    {
      return FlxSort.byValues(desc ? FlxSort.DESCENDING : FlxSort.ASCENDING, a.timeStamp, b.timeStamp);
    });
    return timeChanges;
  }
}
