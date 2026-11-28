import 'dart:async';
import 'package:material_ui/material_ui.dart';
import 'cloud_session.dart';

class CloudPage extends StatefulWidget{
  final CloudSession session;
  final bool vi;
  const CloudPage({super.key,required this.session,required this.vi});
  @override State<CloudPage> createState()=>_CloudPageState();
}
class _CloudPageState extends State<CloudPage>{
  final _origin=TextEditingController(text:'https://openvietgis.cloud');
  String t(String vi,String en)=>widget.vi?vi:en;
  @override void dispose(){_origin.dispose();super.dispose();}
  @override Widget build(BuildContext context)=>ListenableBuilder(
    listenable:widget.session,builder:(context,_){
      final s=widget.session,h=s.health;
      return Scaffold(appBar:AppBar(title:Text(t('Kết nối OpenVietGIS','Connect OpenVietGIS'))),
        body:ListView(padding:const EdgeInsets.all(20),children:[
          Text(t('Máy chủ Cloud hoặc self-host','Cloud or self-hosted server'),style:Theme.of(context).textTheme.titleLarge),
          const SizedBox(height:12),
          TextField(controller:_origin,enabled:!s.busy,keyboardType:TextInputType.url,
            autocorrect:false,decoration:InputDecoration(labelText:t('Địa chỉ HTTPS','HTTPS origin'),border:const OutlineInputBorder())),
          const SizedBox(height:12),
          FilledButton.icon(onPressed:s.busy?null:()=>unawaited(s.check(_origin.text)),
            icon:const Icon(Icons.cloud_sync),label:Text(t('Kiểm tra kết nối','Check connection'))),
          if(s.busy)const LinearProgressIndicator(),
          if(s.error!=null)Text(t('Không kết nối được máy chủ OpenVietGIS.','Unable to reach an OpenVietGIS server.')),
          if(h!=null)Card(child:ListTile(leading:const Icon(Icons.cloud_done),
            title:Text(h.origin.toString()),subtitle:Text('${h.version} · ${h.databaseReady?t('CSDL sẵn sàng','Database ready'):t('CSDL chưa sẵn sàng','Database unavailable')}'))),
          const SizedBox(height:16),
          Text(t('Kiểm tra kết nối không gửi dữ liệu bản đồ hay thông tin đăng nhập. Đăng nhập Mobile và đồng bộ được khóa cho đến khi máy chủ và thiết bị đạt yêu cầu xác thực.',
            'Connection checks do not send map data or credentials. Mobile sign-in and sync remain locked until server and device authentication are qualified.')),
          const SizedBox(height:12),
          OutlinedButton(onPressed:null,child:Text(t('Đăng nhập an toàn','Secure sign-in'))),
          if(h!=null)TextButton(onPressed:s.disconnect,child:Text(t('Ngắt kết nối','Disconnect'))),
        ]));
    });
}
