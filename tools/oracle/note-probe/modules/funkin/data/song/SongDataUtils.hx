package funkin.data.song;

import flixel.util.FlxSort;
import funkin.data.song.SongData.SongTimeChange;

/**
 * Probe shadow of funkin.data.song.SongDataUtils. `sortTimeChanges` is copied
 * verbatim from source/funkin/data/song/SongDataUtils.hx:L266-274 (same as the
 * proven conductor probe).
 */
@:nullSafety
class SongDataUtils
{
  public static function sortTimeChanges(timeChanges:Array<SongTimeChange>, desc:Bool = false):Array<SongTimeChange>
  {
    timeChanges.sort(function(a:SongTimeChange, b:SongTimeChange):Int
    {
      return FlxSort.byValues(desc ? FlxSort.DESCENDING : FlxSort.ASCENDING, a.timeStamp, b.timeStamp);
    });
    return timeChanges;
  }
}
