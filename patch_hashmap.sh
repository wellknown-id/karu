cat << 'PATCH' > /tmp/hashmap.diff
--- crates/karu/src/path.rs
+++ crates/karu/src/path.rs
@@ -134,8 +134,7 @@
                 PathSegment::Index(idx) => current.get(idx)?,
                 PathSegment::Variable(_) => {
                     // Fall back to full resolver with empty bindings
-                    let bindings = HashMap::new();
-                    return self.resolve_with_bindings(value, &bindings);
+                    return self.resolve_with_bindings(value, &HashMap::new());
                 }
             };
         }
PATCH
patch -p0 < /tmp/hashmap.diff
