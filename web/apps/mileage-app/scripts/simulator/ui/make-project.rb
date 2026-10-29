#!/usr/bin/env ruby
# Writes MileageDrive.xcodeproj: one UI-testing bundle that drives an app that is already installed
# in the Simulator, by its bundle id. Uses the `xcodeproj` gem that CocoaPods installs.
#
#   ruby make-project.rb OUT_DIR SWIFT_FILE
#
# (docs/spec/mobile.md 8.8. A test tool for the Mac; not part of the app, its build or any image.)
require 'fileutils'
require 'xcodeproj'

out, swift = ARGV
abort('usage: make-project.rb OUT_DIR SWIFT_FILE') unless out && swift
abort("no such file: #{swift}") unless File.file?(swift)

FileUtils.mkdir_p(out)
project = Xcodeproj::Project.new(File.join(out, 'MileageDrive.xcodeproj'))
target = project.new_target(:ui_test_bundle, 'MileageDrive', :ios, '16.4', nil, :swift)
target.add_file_references([project.main_group.new_file(File.expand_path(swift))])
target.build_configurations.each do |config|
  config.build_settings.merge!(
    'PRODUCT_BUNDLE_IDENTIFIER' => 'org.obop.mileage.drive',
    'SWIFT_VERSION' => '5.0',
    'GENERATE_INFOPLIST_FILE' => 'YES',
    'CODE_SIGNING_ALLOWED' => 'NO',
    'TARGETED_DEVICE_FAMILY' => '1'
  )
end
project.save

scheme = Xcodeproj::XCScheme.new
scheme.add_test_target(target)
scheme.save_as(project.path, 'MileageDrive', true)
puts File.join(out, 'MileageDrive.xcodeproj')
