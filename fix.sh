cat << 'PATCH' > /tmp/path_resolve.diff
--- crates/karu/src/path.rs
+++ crates/karu/src/path.rs
@@ -134,8 +134,7 @@
                 PathSegment::Index(idx) => current.get(idx)?,
                 PathSegment::Variable(_) => {
                     // Fall back to full resolver with empty bindings
-                    let mut bindings = HashMap::new();
-                    return self.resolve_with_bindings(value, &mut bindings);
+                    return self.resolve_with_bindings(value, &mut HashMap::new());
                 }
             };
         }
PATCH
patch -p0 < /tmp/path_resolve.diff
cat << 'PATCH' > /tmp/rule_simple.diff
--- crates/karu/src/rule.rs
+++ crates/karu/src/rule.rs
@@ -446,8 +446,7 @@

         // For PathRef patterns, fall back to the full path (rare case)
         if let Pattern::PathRef(_) = &self.pattern {
-            let mut bindings = std::collections::HashMap::new();
-            return self.evaluate_simple(input, &mut bindings);
+            return self.evaluate_simple(input, &mut std::collections::HashMap::new());
         }

         self.dispatch_op(data, &self.pattern, input)
PATCH
patch -p0 < /tmp/rule_simple.diff
cat << 'PATCH' > /tmp/hashmap.diff
--- crates/karu/src/rule.rs
+++ crates/karu/src/rule.rs
@@ -388,8 +388,7 @@
         if self.quantifier.is_none() {
             return self.evaluate_fast(input);
         }
-        let mut bindings = std::collections::HashMap::new();
-        self.evaluate_with_bindings(input, &mut bindings)
+        self.evaluate_with_bindings(input, &mut std::collections::HashMap::new())
     }

     /// Evaluate this condition with variable bindings.
PATCH
patch -p0 < /tmp/hashmap.diff
