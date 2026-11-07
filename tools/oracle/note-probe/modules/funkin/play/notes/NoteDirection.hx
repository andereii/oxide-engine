package funkin.play.notes;

/**
 * Probe shadow of funkin.play.notes.NoteDirection: the four direction values
 * and the Int conversions are copied VERBATIM from
 * source/funkin/play/notes/NoteDirection.hx:L10-33; the color/name helpers are
 * dropped because the retained note path never reads them.
 */
enum abstract NoteDirection(Int) from Int to Int
{
  public var LEFT = 0;
  public var DOWN = 1;
  public var UP = 2;
  public var RIGHT = 3;

  @:from
  public static function fromInt(value:Int):NoteDirection
  {
    return switch (value % 4)
    {
      case 0:
        LEFT;
      case 1:
        DOWN;
      case 2:
        UP;
      case 3:
        RIGHT;
      default:
        LEFT;
    }
  }

  public function toString():String
  {
    return switch (abstract)
    {
      case LEFT:
        'left';
      case DOWN:
        'down';
      case UP:
        'up';
      case RIGHT:
        'right';
      default:
        'unknown';
    }
  }
}
