//! Text recognition with Vision, the system's own, on the machine.

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::msg_send;
use objc2_core_foundation::{CFData, CFRetained, CGRect};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSString};

use crate::place::Line;
use crate::text::Region;
use crate::{CaptureError, Frame};

#[link(name = "Vision", kind = "framework")]
unsafe extern "C" {}

/// `VNRequestTextRecognitionLevelAccurate`.
const ACCURATE: isize = 0;

/// The frame's pixels as BGRA without alpha, the layout of the capture.
fn bitmap_info() -> CGBitmapInfo {
    CGBitmapInfo(CGImageAlphaInfo::NoneSkipFirst.0 | CGImageByteOrderInfo::Order32Little.0)
}

/// A region of the frame as an image Vision reads.
fn image(frame: &Frame, (left, top, width, height): (usize, usize, usize, usize)) -> Option<CFRetained<CGImage>> {
    if width == 0 || height == 0 {
        return None;
    }
    let row = frame.width as usize * 4;
    let mut bytes = Vec::with_capacity(width * height * 4);
    for y in top..top + height {
        let start = y * row + left * 4;
        bytes.extend_from_slice(frame.bgra.get(start..start + width * 4)?);
    }
    let data = CFData::from_bytes(&bytes);
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    let space = CGColorSpace::new_device_rgb()?;
    // SAFETY: the provider holds width × height pixels of four bytes, rows
    // packed, as the size and layout given say; no decode array.
    unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            width * 4,
            Some(&space),
            bitmap_info(),
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}

pub fn lines(frame: &Frame, region: Region) -> Result<Vec<Line>, CaptureError> {
    let failed = |what: &str| CaptureError::Failed(format!("text recognition: {what}"));
    let (left, top, width, height) = region.pixels(frame);
    let image = image(frame, (left, top, width, height)).ok_or_else(|| failed("empty region"))?;
    let request_class = AnyClass::get(c"VNRecognizeTextRequest").ok_or_else(|| failed("Vision missing"))?;
    let handler_class = AnyClass::get(c"VNImageRequestHandler").ok_or_else(|| failed("Vision missing"))?;
    // SAFETY: messages of VNRecognizeTextRequest, VNImageRequestHandler and
    // their results, as Vision documents them, with arguments of their types.
    unsafe {
        let request: Retained<AnyObject> = msg_send![request_class, new];
        let _: () = msg_send![&*request, setRecognitionLevel: ACCURATE];
        let _: () = msg_send![&*request, setUsesLanguageCorrection: false];
        let languages = NSArray::from_retained_slice(&[NSString::from_str("fr-FR"), NSString::from_str("en-US")]);
        let _: () = msg_send![&*request, setRecognitionLanguages: &*languages];
        let options = NSDictionary::<NSString, AnyObject>::new();
        let allocated: Allocated<AnyObject> = msg_send![handler_class, alloc];
        let handler: Retained<AnyObject> = msg_send![allocated, initWithCGImage: &*image, options: &*options];
        let requests = NSArray::from_retained_slice(std::slice::from_ref(&request));
        let done: Result<(), Retained<NSError>> = msg_send![&*handler, performRequests: &*requests, error: _];
        done.map_err(|e| failed(&e.localizedDescription().to_string()))?;
        let results: Option<Retained<NSArray<AnyObject>>> = msg_send![&*request, results];
        let mut lines = Vec::new();
        for observation in results.iter().flat_map(|r| r.iter()) {
            let candidates: Retained<NSArray<AnyObject>> = msg_send![&*observation, topCandidates: 1usize];
            let Some(top_candidate) = candidates.firstObject() else { continue };
            let text: Retained<NSString> = msg_send![&*top_candidate, string];
            let confidence: f32 = msg_send![&*top_candidate, confidence];
            // Normalised, from the bottom left of the region.
            let b: CGRect = msg_send![&*observation, boundingBox];
            let (w, h) = (width as f64, height as f64);
            lines.push(Line {
                text: text.to_string(),
                confidence,
                bounds: (
                    (left as f64 + b.origin.x * w) as f32,
                    (top as f64 + (1.0 - b.origin.y - b.size.height) * h) as f32,
                    (b.size.width * w) as f32,
                    (b.size.height * h) as f32,
                ),
            });
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::place::{place, Place};
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFString, CFType, CGPoint, CGSize};
    use objc2_core_graphics::{CGBitmapContextCreate, CGContext};
    use objc2_core_text::{kCTFontAttributeName, kCTForegroundColorFromContextAttributeName, CTFont, CTLine};

    /// A frame with `lines` written in white on a dark ground, each at its
    /// left and top in pixels.
    fn written(size: (u32, u32), lines: &[(&str, f64, f64)]) -> Frame {
        let (w, h) = (size.0 as usize, size.1 as usize);
        let mut bgra = vec![0u8; w * h * 4];
        let space = CGColorSpace::new_device_rgb().unwrap();
        // SAFETY: the buffer holds w × h pixels of four bytes and outlives the
        // context; Core Text draws the lines into it.
        unsafe {
            let context = CGBitmapContextCreate(bgra.as_mut_ptr().cast(), w, h, 8, w * 4, Some(&space), bitmap_info().0)
                .expect("a bitmap context");
            CGContext::set_rgb_fill_color(Some(&context), 0.12, 0.1, 0.16, 1.0);
            CGContext::fill_rect(Some(&context), CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as f64, h as f64)));
            CGContext::set_rgb_fill_color(Some(&context), 0.95, 0.93, 0.85, 1.0);
            let font = CTFont::with_name(&CFString::from_str("Helvetica"), 26.0, std::ptr::null());
            let from_context: &CFType = CFBoolean::new(true);
            let attributes = CFDictionary::<CFString, CFType>::from_slices(
                &[kCTFontAttributeName, kCTForegroundColorFromContextAttributeName],
                &[&font, from_context],
            );
            for (text, left, top) in lines {
                let string = objc2_core_foundation::CFAttributedString::new(
                    None,
                    Some(&CFString::from_str(text)),
                    Some(attributes.as_opaque()),
                )
                .expect("an attributed string");
                let line = CTLine::with_attributed_string(&string);
                CGContext::set_text_position(Some(&context), *left, h as f64 - top - 26.0);
                line.draw(&context);
            }
        }
        Frame { window: 1, width: size.0, height: size.1, bgra }
    }

    /// Vision reads back what Core Text wrote, accents and minus signs
    /// included, and the place is found in it.
    #[test]
    fn the_place_written_on_a_frame_is_read() {
        let frame = written((900, 300), &[("Plaine des Scarafeuilles", 24.0, 20.0), ("4,-12", 24.0, 62.0)]);
        let read = lines(&frame, Region::ALL).expect("text recognised");
        let found = place(&read);
        assert_eq!(found, Some(Place { coords: (4, -12), subarea: Some("Plaine des Scarafeuilles".into()) }), "{read:?}");
        let frame = written((900, 300), &[("Île de Moon (−3, 7)", 24.0, 120.0)]);
        let read = lines(&frame, Region::ALL).expect("text recognised");
        assert_eq!(place(&read).map(|p| p.coords), Some((-3, 7)), "{read:?}");
    }

    #[test]
    #[ignore = "measurement, run on demand"]
    fn measure_time() {
        let frame = written((1920, 1080), &[("Plaine des Scarafeuilles", 24.0, 20.0), ("4,-12", 24.0, 62.0)]);
        for _ in 0..4 {
            let start = std::time::Instant::now();
            let read = lines(&frame, Region::TOP_LEFT).unwrap();
            println!("{} lines in {:?}", read.len(), start.elapsed());
        }
    }

    /// Only the region asked for is read, and bounds are in frame pixels.
    #[test]
    fn a_region_is_read_alone_in_frame_pixels() {
        let frame = written((1200, 600), &[("12,4", 40.0, 30.0), ("Bonta", 900.0, 400.0)]);
        let read = lines(&frame, Region::TOP_LEFT).expect("text recognised");
        assert_eq!(read.len(), 1, "{read:?}");
        let (left, top, _, height) = read[0].bounds;
        assert!((left - 40.0).abs() < 12.0 && (top - 30.0).abs() < 14.0 && height > 10.0, "{read:?}");
    }
}
