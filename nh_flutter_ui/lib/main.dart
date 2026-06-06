// The original content is temporarily commented out to allow generating a self-contained demo - feel free to uncomment later.

// import 'dart:convert';
// import 'package:flutter/material.dart';
// // 注意：以下导入路径可能需要根据你实际生成的包名微调 (nh_flutter_ui 或 nh_bridge)
// import 'package:nh_flutter_ui/src/rust/api/gallery.dart';
// import 'package:nh_flutter_ui/src/rust/frb_generated.dart';
//
// Future<void> main() async {
//   // 关键：确保在 Flutter 引擎初始化后加载 Rust 的动态库
//   WidgetsFlutterBinding.ensureInitialized();
//   await RustLib.init();
//
//   runApp(const MyApp());
// }
//
// class MyApp extends StatelessWidget {
//   const MyApp({super.key});
//
//   @override
//   Widget build(BuildContext context) {
//     return MaterialApp(
//       title: 'nH-Viewer Debug',
//       theme: ThemeData(
//         brightness: Brightness.dark,
//         primarySwatch: Colors.pink,
//       ),
//       home: const GalleryListScreen(),
//     );
//   }
// }
//
// class GalleryListScreen extends StatefulWidget {
//   const GalleryListScreen({super.key});
//
//   @override
//   State<GalleryListScreen> createState() => _GalleryListScreenState();
// }
//
// class _GalleryListScreenState extends State<GalleryListScreen> {
//   // 直接持有 Rust 返回的 List
//   late Future<List<GalleryPreview>> _galleriesFuture;
//
//   @override
//   void initState() {
//     super.initState();
//     // 调用我们在 Rust 写的 Mock 接口
//     _galleriesFuture = nhListGalleries(limit: 20, offset: 0);
//   }
//
//   @override
//   Widget build(BuildContext context) {
//     return Scaffold(
//       appBar: AppBar(title: const Text('Rust Mock 数据测试')),
//       body: FutureBuilder<List<GalleryPreview>>(
//         future: _galleriesFuture,
//         builder: (context, snapshot) {
//           if (snapshot.connectionState == ConnectionState.waiting) {
//             return const Center(child: CircularProgressIndicator());
//           } else if (snapshot.hasError) {
//             return Center(child: Text('出错了: ${snapshot.error}'));
//           } else if (!snapshot.hasData || snapshot.data!.isEmpty) {
//             return const Center(child: Text('没有数据'));
//           }
//
//           final galleries = snapshot.data!;
//           return ListView.builder(
//             itemCount: galleries.length,
//             itemBuilder: (context, index) {
//               final gallery = galleries[index];
//               return Card(
//                 margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
//                 child: ListTile(
//                   leading: const Icon(Icons.book, size: 40), // 暂时代替封面图
//                   title: Text(gallery.title),
//                   subtitle: Text('ID: ${gallery.id} | Media: ${gallery.mediaId}'),
//                   trailing: const Icon(Icons.arrow_forward_ios, size: 16),
//                   onTap: () => _showGalleryDetails(context, gallery.id),
//                 ),
//               );
//             },
//           );
//         },
//       ),
//     );
//   }
//
//   // 点击列表项时，调用详情接口并解析 JSON
//   void _showGalleryDetails(BuildContext context, int id) async {
//     try {
//       final detail = await nhGetGallery(id: id);
//
//       // 优雅地反序列化 JSON 字符串
//       List<dynamic> tagsList = jsonDecode(detail.tagsJson);
//       String tagNames = tagsList.map((t) => t['name'].toString()).join(', ');
//
//       if (context.mounted) {
//         showDialog(
//           context: context,
//           builder: (context) => AlertDialog(
//             title: Text(detail.titlePretty),
//             content: Column(
//               mainAxisSize: MainAxisSize.min,
//               crossAxisAlignment: CrossAxisAlignment.start,
//               children: [
//                 Text('❤️ 收藏: ${detail.numFavorites}'),
//                 Text('📄 页数: ${detail.numPages}'),
//                 const SizedBox(height: 12),
//                 const Text('🏷️ 标签:', style: TextStyle(fontWeight: FontWeight.bold)),
//                 Text(tagNames),
//               ],
//             ),
//             actions: [
//               TextButton(
//                 onPressed: () => Navigator.pop(context),
//                 child: const Text('关闭'),
//               )
//             ],
//           ),
//         );
//       }
//     } catch (e) {
//       debugPrint('获取详情异常: $e');
//     }
//   }
// }

import 'package:flutter/material.dart';
import 'package:nh_flutter_ui/src/rust/api/simple.dart';
import 'package:nh_flutter_ui/src/rust/frb_generated.dart';

Future<void> main() async {
  await RustLib.init();
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(title: const Text('flutter_rust_bridge quickstart')),
        body: Center(
          child: Text(
            'Action: Call Rust `greet("Tom")`\nResult: `${greet(name: "Tom")}`',
          ),
        ),
      ),
    );
  }
}
