import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nh_flutter_ui/src/rust/api/gallery.dart';
import 'package:nh_flutter_ui/src/rust/api/simple.dart';
import 'package:nh_flutter_ui/src/rust/frb_generated.dart';
import 'package:nh_flutter_ui/src/providers/gallery_provider.dart';
import 'package:nh_flutter_ui/src/screens/settings_screen.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();

  runApp(const ProviderScope(child: MyApp()));
}

class MyApp extends ConsumerWidget {
  const MyApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return MaterialApp(
      title: 'nH-Viewer',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        brightness: Brightness.dark,
        colorSchemeSeed: Colors.pink,
        useMaterial3: true,
      ),
      home: const HomeScreen(),
    );
  }
}

class HomeScreen extends ConsumerStatefulWidget {
  const HomeScreen({super.key});

  @override
  ConsumerState<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends ConsumerState<HomeScreen> {
  final _searchController = TextEditingController();

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  void _onSearch() {
    final query = _searchController.text.trim();
    if (query.isNotEmpty) {
      ref.read(searchProvider.notifier).search(query);
    }
  }

  @override
  Widget build(BuildContext context) {
    final searchState = ref.watch(searchProvider);

    return Scaffold(
      appBar: AppBar(
        title: const Text('nH-Viewer'),
        actions: [
          IconButton(
            icon: const Icon(Icons.settings),
            tooltip: '设置',
            onPressed: () {
              Navigator.push(
                context,
                MaterialPageRoute(
                  builder: (_) => const SettingsScreen(),
                ),
              );
            },
          ),
        ],
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(56),
          child: Padding(
            padding: const EdgeInsets.all(8.0),
            child: TextField(
              controller: _searchController,
              decoration: InputDecoration(
                hintText: '搜索画廊...',
                prefixIcon: const Icon(Icons.search),
                suffixIcon: IconButton(
                  icon: const Icon(Icons.clear),
                  onPressed: () {
                    _searchController.clear();
                    ref.read(searchProvider.notifier).clear();
                  },
                ),
                border: OutlineInputBorder(
                  borderRadius: BorderRadius.circular(12),
                ),
                filled: true,
                fillColor: Theme.of(context).colorScheme.surface,
              ),
              onSubmitted: (_) => _onSearch(),
            ),
          ),
        ),
      ),
      body: _buildBody(searchState),
    );
  }

  Widget _buildBody(SearchState searchState) {
    // 初始状态：显示欢迎信息
    if (searchState.query.isEmpty &&
        searchState.results.isEmpty &&
        !searchState.isLoading) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              Icons.search,
              size: 64,
              color: Theme.of(context).colorScheme.primary.withValues(alpha: 0.5),
            ),
            const SizedBox(height: 16),
            Text(
              '输入关键词开始搜索',
              style: Theme.of(context).textTheme.titleMedium?.copyWith(
                    color: Theme.of(context)
                        .colorScheme
                        .onSurface
                        .withValues(alpha: 0.6),
                  ),
            ),
            const SizedBox(height: 8),
            Text(
              'Rust Bridge: ${greet(name: "nH-Viewer")}',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ),
      );
    }

    // 错误状态
    if (searchState.error != null && searchState.results.isEmpty) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Icon(Icons.error_outline, size: 48, color: Colors.red),
            const SizedBox(height: 16),
            Text('搜索出错: ${searchState.error}'),
            const SizedBox(height: 8),
            ElevatedButton(
              onPressed: _onSearch,
              child: const Text('重试'),
            ),
          ],
        ),
      );
    }

    // 搜索结果列表
    return ListView.builder(
      itemCount: searchState.results.length + (searchState.isLoading ? 1 : 0),
      itemBuilder: (context, index) {
        // 加载指示器
        if (index == searchState.results.length) {
          return const Center(
            child: Padding(
              padding: EdgeInsets.all(16.0),
              child: CircularProgressIndicator(),
            ),
          );
        }

        final gallery = searchState.results[index];
        return GalleryPreviewCard(
          gallery: gallery,
          onTap: () => _showGalleryDetails(context, gallery.id),
        );
      },
    );
  }

  void _showGalleryDetails(BuildContext context, BigInt id) {
    Navigator.push(
      context,
      MaterialPageRoute(
        builder: (_) => GalleryDetailScreen(galleryId: id),
      ),
    );
  }
}

/// 画廊预览卡片组件
class GalleryPreviewCard extends StatelessWidget {
  final GalleryPreviewInfo gallery;
  final VoidCallback onTap;

  const GalleryPreviewCard({
    super.key,
    required this.gallery,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      child: ListTile(
        leading: gallery.coverUrl != null
            ? ClipRRect(
                borderRadius: BorderRadius.circular(4),
                child: Image.network(
                  gallery.coverUrl!,
                  width: 48,
                  height: 64,
                  fit: BoxFit.cover,
                  errorBuilder: (_, __, ___) =>
                      const Icon(Icons.broken_image, size: 40),
                ),
              )
            : const Icon(Icons.book, size: 40),
        title: Text(
          gallery.title,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
        ),
        subtitle: Text(
          'ID: ${gallery.id} | ${gallery.numPages} 页',
        ),
        trailing: const Icon(Icons.arrow_forward_ios, size: 16),
        onTap: onTap,
      ),
    );
  }
}

/// 画廊详情页面（使用 Riverpod 的 AsyncNotifierWidget 模式）
class GalleryDetailScreen extends ConsumerWidget {
  final BigInt galleryId;

  const GalleryDetailScreen({super.key, required this.galleryId});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final galleryAsync = ref.watch(galleryDetailProvider(galleryId));

    return Scaffold(
      appBar: AppBar(
        title: galleryAsync.whenOrNull(
          data: (g) => Text(g.titlePretty ?? g.titleEn ?? '画廊详情'),
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () =>
                ref.read(galleryDetailProvider(galleryId).notifier).refresh(),
          ),
        ],
      ),
      body: galleryAsync.when(
        loading: () => const Center(child: CircularProgressIndicator()),
        error: (error, stack) => Center(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(Icons.error_outline, size: 48, color: Colors.red),
              const SizedBox(height: 16),
              Text('加载失败: $error'),
              const SizedBox(height: 8),
              ElevatedButton(
                onPressed: () => ref
                    .read(galleryDetailProvider(galleryId).notifier)
                    .refresh(),
                child: const Text('重试'),
              ),
            ],
          ),
        ),
        data: (gallery) => _buildContent(context, gallery),
      ),
    );
  }

  Widget _buildContent(BuildContext context, GalleryInfo gallery) {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // 封面图
          if (gallery.coverUrl != null)
            Center(
              child: ClipRRect(
                borderRadius: BorderRadius.circular(8),
                child: Image.network(
                  gallery.coverUrl!,
                  height: 300,
                  fit: BoxFit.contain,
                  errorBuilder: (_, __, ___) => Container(
                    height: 300,
                    width: double.infinity,
                    color: Colors.grey[800],
                    child: const Icon(Icons.broken_image, size: 64),
                  ),
                ),
              ),
            ),
          const SizedBox(height: 16),

          // 标题
          if (gallery.titlePretty != null)
            Text(
              gallery.titlePretty!,
              style: Theme.of(context).textTheme.headlineSmall,
            ),
          if (gallery.titleEn != null) ...[
            const SizedBox(height: 4),
            Text(
              gallery.titleEn!,
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                    color: Theme.of(context)
                        .colorScheme
                        .onSurface
                        .withValues(alpha: 0.7),
                  ),
            ),
          ],
          const SizedBox(height: 16),

          // 统计信息
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceAround,
                children: [
                  _StatItem(
                    icon: Icons.pages,
                    label: '页数',
                    value: '${gallery.numPages}',
                  ),
                  _StatItem(
                    icon: Icons.favorite,
                    label: '收藏',
                    value: '${gallery.numFavorites}',
                  ),
                  _StatItem(
                    icon: Icons.tag,
                    label: 'ID',
                    value: '${gallery.id}',
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// 统计信息小部件
class _StatItem extends StatelessWidget {
  final IconData icon;
  final String label;
  final String value;

  const _StatItem({
    required this.icon,
    required this.label,
    required this.value,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(icon, color: Theme.of(context).colorScheme.primary),
        const SizedBox(height: 4),
        Text(value, style: Theme.of(context).textTheme.titleMedium),
        Text(label, style: Theme.of(context).textTheme.bodySmall),
      ],
    );
  }
}