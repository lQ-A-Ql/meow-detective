# Linux/Kubernetes 样本外部工具交叉核验

日期：2026-09-24  
样本：`E:\shuzheng_k8s\服务器检材01.E01`  
工具：libewf 20260615、Sleuth Kit 4.15.0、WSL2 Kali 的 LVM/XFS 工具

## 结论分类

| 项目 | 外部工具证据 | Meow~Detective 现状 | 结论 |
|---|---|---|---|
| EWF 是否可读、介质大小 | `ewfinfo`：FTK EWF、40 GiB、512B sector；`img_stat`：EWF、42949672960 bytes、MD5 `693ba49b29f7e57fed8dd111c07588f9` | 能导入并建立 source DB | 样本存在，基础导入能力正常 |
| 分区结构 | `mmls`：1 MiB Linux 分区；其后 38 GiB Linux LVM | 当前 topology 可形成 OS/host scope | 样本存在；TSK 看到的是 LVM 容器，不是直接文件系统 |
| 根文件系统类型 | WSL 通过 `losetup`、LVM 激活、`xfs_info` 确认为 XFS；只读 `norecovery,nouuid` 挂载成功 | Linux 文件树已能导入一部分 | 样本存在；TSK `fsstat` 直接对分区失败是 LVM/XFS 路径限制，不是样本缺失 |
| 主机名 | 外部只读挂载 `/etc/hostname` = `master` | 页面此前显示 `—` | 后端/前端证据链缺口，非样本缺失 |
| OS 与内核 | `/etc/os-release` = CentOS Linux 7；`/boot/vmlinuz-3.10.0-1160.el7.x86_64` 存在 | 页面此前显示 `—` | 后端或 summary 未接入已有文件证据，非样本缺失 |
| 主机地址与节点关系 | `/etc/hosts` 含 `192.168.50.80 master`、`.81 node1`、`.82 node2` | 网络证据面为空 | 样本存在；解析/投影/前端展示缺口，需要逐层定位 |
| Kubernetes 控制面 | 外部列出 `admin.conf`、`controller-manager.conf`、`kubelet.conf`、`scheduler.conf`、`manifests/etcd.yaml`、`kube-apiserver.yaml`、`kube-controller-manager.yaml`、`kube-scheduler.yaml` | 当前 parser 发现 admin.conf、kube-apiserver.yaml、etcd snapshot，未展示完整控制面服务集合 | 样本存在；Kubernetes artifact discovery/parser/UI 覆盖不足 |
| Kubernetes PKI | 外部列出 apiserver、etcd、CA、front-proxy、service-account 等证书和密钥文件 | 当前未在集群卡片展示 PKI fingerprint/证书集合 | 样本存在；PKI closure 尚未实现/未展示 |
| etcd snapshot | 外部文件存在，大小 8 MiB；应用 parser 报 `no valid Bolt meta page` | 页面显示 candidate/partial | 样本存在；当前 parser 失败，不能把它写成“样本缺失” |
| Docker 容器 | 外部只读挂载发现 `/var/lib/docker/containers/` 下大量 container ID、`config.v2.json`、`hostconfig.json`、json log | 当前工作负载面没有容器名称/镜像/服务列表 | 样本存在；容器 runtime parser/前端展示缺失 |
| `ewfverify` | 已启动并输出到 14.6%，因耗时停止，未形成完整校验结论 | 不应标为完整外部 hash 校验 | 外部验证未完成，不能据此宣称 EWF 全量校验通过 |

## TSK 与 libewf 的边界

`mmls` 能证明镜像分区和 LVM 布局。直接执行 `fsstat -i ewf -o 2048` 不能识别文件系统，因为该分区不是直接 XFS，而是 LVM PV；将 LVM PV 直接交给 TSK 也不能穿透 LVM。随后在 WSL 中通过只读 loop/LVM/XFS 路径挂载同一证据，证明文件和目录确实存在。这个失败应归类为工具路径/能力边界，不应归类为样本缺失。

## 对截图问题的判断

1. 同一个 E01 出现为 Kubernetes、OS、Physical Host 多张平级卡片，是前端直接按 scope 渲染造成的层级丢失。现在改为一个数据源卡片，内部显示证据层级。
2. 主机名、CentOS 7、3.10.0-1160 内核和 `192.168.50.80/81/82` 已被外部工具和只读挂载证明存在；此前空白属于后端事实接入或前端显示缺口。
3. Kubernetes 控制面清单和 PKI 文件已被外部工具证明存在；当前系统只解析其中一部分，属于 parser/analysis coverage 缺口。
4. Docker 容器目录已被外部工具证明存在；当前工作负载面没有展示容器身份，属于 runtime metadata parser/UI 缺口。
5. etcd snapshot 文件存在，但当前 Bolt parser 失败；应显示“文件存在、解析失败”，不能显示为“未发现”。

## 当前不应下的结论

- 不能仅凭静态 `/etc/hosts` 证明当时节点在线或网络连通。
- 不能仅凭静态 manifest 证明 Pod 当时正在运行。
- 不能把 `master` 文本直接升级为已交叉验证的 Kubernetes control-plane identity；目前是强候选，需要 node UID、etcd member ID、PKI 或其他独立证据闭合。
