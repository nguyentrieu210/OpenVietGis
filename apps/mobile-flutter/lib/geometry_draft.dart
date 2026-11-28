/// Editable paths of a GeoJSON XY geometry. Polygon closing coordinates are
/// regenerated on export; all parts and interior rings retain their order.
class GeometryDraft {
  final String type;
  final List<List<List<double>>> paths;
  final List<int> polygonRingCounts;
  GeometryDraft._(this.type,this.paths,this.polygonRingCounts);
  bool get rings=>type=='Polygon'||type=='MultiPolygon';
  int get minimum=>rings?3:type=='LineString'||type=='MultiLineString'?2:1;
  int get vertexCount=>paths.fold<int>(0,(n,p)=>n+p.length+(rings?1:0));
  static GeometryDraft parse(Map<String,dynamic> g){
    final type=g['type'] as String;final raw=g['coordinates'];
    final paths=<List<List<double>>>[];final counts=<int>[];
    List<double> xy(dynamic p){
      if(p is! List||p.length!=2||p.any((v)=>v is! num))throw FormatException('XY required');
      final x=(p[0] as num).toDouble(),y=(p[1] as num).toDouble();
      if(!x.isFinite||!y.isFinite||x.abs()>180||y.abs()>90)throw FormatException('Invalid XY');
      return [x,y];
    }
    void path(dynamic p,{bool ring=false,int min=1}){
      if(p is! List||p.length<min+(ring?1:0))throw FormatException('Invalid path');
      final points=p.map(xy).toList();
      if(ring){if(points.first[0]!=points.last[0]||points.first[1]!=points.last[1])throw FormatException('Open ring');points.removeLast();}
      paths.add(points);
    }
    void polygon(dynamic p){
      if(p is! List||p.isEmpty)throw FormatException('Invalid polygon');
      counts.add(p.length);for(final r in p){path(r,ring:true,min:3);}
    }
    switch(type){
      case 'Point':path([raw]);break;
      case 'MultiPoint':path(raw);break;
      case 'LineString':path(raw,min:2);break;
      case 'MultiLineString':for(final p in raw as List){path(p,min:2);}break;
      case 'Polygon':polygon(raw);break;
      case 'MultiPolygon':for(final p in raw as List){polygon(p);}break;
      default:throw FormatException('Unsupported geometry');
    }
    final d=GeometryDraft._(type,paths,counts);
    if(paths.isEmpty||paths.length>256||d.vertexCount>4096)throw FormatException('Geometry limit');
    return d;
  }
  Map<String,dynamic> toGeoJson(){
    if(vertexCount>4096||paths.any((p)=>p.length<minimum))throw FormatException('Geometry limit');
    final values=paths.map((p)=>[...p.map((xy)=>List<double>.from(xy)),if(rings)List<double>.from(p.first)]).toList();
    dynamic coordinates;
    switch(type){
      case 'Point':if(values.single.length!=1)throw FormatException('Point count');coordinates=values.single.single;break;
      case 'MultiPoint':case 'LineString':coordinates=values.single;break;
      case 'MultiLineString':case 'Polygon':coordinates=values;break;
      case 'MultiPolygon':
        var at=0;final polygons=<List<List<List<double>>>>[];
        for(final count in polygonRingCounts){polygons.add(values.sublist(at,at+count));at+=count;}
        coordinates=polygons;break;
      default:throw FormatException('Unsupported geometry');
    }
    return {'type':type,'coordinates':coordinates};
  }
}
