import 'dart:async';
import 'dart:convert';
/// SSE is a wake-up hint, not authoritative delta or cursor. Bound line and
/// frame sizes before UTF-8 decode; no unbounded event buffer.
final class EventHint {
  final String event,data;
  const EventHint(this.event,this.data);
  bool get revoked=>event=='session_revoked';
}
Stream<EventHint> hints(Stream<List<int>> input,{required bool Function() active})async*{
  var line=<int>[];var data=<String>[];var event='message';var bytes=0;
  await for(final chunk in input.timeout(const Duration(seconds:45))){
    if(!active())return;
    for(final b in chunk){
      if(b!=10){if(line.length>=8192)throw StateError('SSE_LINE_LIMIT');line.add(b);continue;}
      if(line.isNotEmpty&&line.last==13)line.removeLast();
      final text=utf8.decode(line);line=<int>[];
      if(text.isEmpty){
        if(data.isNotEmpty){if(!active())return;final hint=EventHint(event,data.join('\n'));yield hint;if(hint.revoked)return;}
        data=[];event='message';bytes=0;continue;
      }
      if(text.startsWith(':'))continue;
      bytes+=text.length;if(bytes>32768)throw StateError('SSE_FRAME_LIMIT');
      final split=text.indexOf(':');final name=split<0?text:text.substring(0,split);
      var value=split<0?'':text.substring(split+1);if(value.startsWith(' '))value=value.substring(1);
      if(name=='event')event=value;
      if(name=='data')data.add(value);
    }
  }
}
